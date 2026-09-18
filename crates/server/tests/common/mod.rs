//! 集成测试共用脚手架：临时数据目录 + 真实 HTTP 栈 + 会话管理。

#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{Value, json};
use tower::ServiceExt;

use circulation_server::config::Config;
use circulation_server::{bootstrap, build_app};

pub struct TestApp {
    router: Router,
    _dir: tempfile::TempDir,
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub body: Value,
    pub set_cookie: Option<String>,
}

pub struct Session {
    cookie: String,
}

impl TestApp {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let config = Config {
            data_dir: dir.path().to_path_buf(),
            ..Config::default()
        };
        config.ensure_dirs().unwrap();

        let state = bootstrap(config).expect("初始化失败");
        let router = build_app(state).expect("构建路由失败");

        Self { router, _dir: dir }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<Session, Response> {
        let response = self
            .post(
                "/api/auth/login",
                json!({ "username": username, "password": password }),
                None,
            )
            .await;

        if response.status != StatusCode::OK {
            return Err(response);
        }

        Ok(Session {
            cookie: response.set_cookie.expect("登录响应缺少 Set-Cookie"),
        })
    }

    /// 建好管理员会话，并把强制改密走完，方便直接测业务接口。
    pub async fn admin(&self) -> Session {
        let session = self.login("admin", "admin123").await.expect("管理员登录失败");
        let changed = self
            .post(
                "/api/auth/change-password",
                json!({ "oldPassword": "admin123", "newPassword": "admin12345" }),
                Some(&session),
            )
            .await;
        assert_eq!(changed.status, StatusCode::OK, "改密失败: {:?}", changed.body);

        self.login("admin", "admin12345")
            .await
            .expect("改密后登录失败")
    }

    pub async fn get(&self, uri: &str, session: Option<&Session>) -> Response {
        let mut builder = Request::builder().method("GET").uri(uri);
        if let Some(session) = session {
            builder = builder.header("cookie", session.cookie_header());
        }
        self.call(builder.body(Body::empty()).unwrap()).await
    }

    pub async fn post(&self, uri: &str, payload: Value, session: Option<&Session>) -> Response {
        self.send_json("POST", uri, payload, session).await
    }

    pub async fn put(&self, uri: &str, payload: Value, session: Option<&Session>) -> Response {
        self.send_json("PUT", uri, payload, session).await
    }

    pub async fn delete(&self, uri: &str, session: &Session) -> Response {
        let builder = Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("cookie", session.cookie_header());
        self.call(builder.body(Body::empty()).unwrap()).await
    }

    /// 手工拼 multipart 请求体，走真实的文件上传路径。
    pub async fn upload(
        &self,
        uri: &str,
        file_name: &str,
        content: &[u8],
        session: &Session,
    ) -> Response {
        const BOUNDARY: &str = "----circulationTestBoundary";

        let mut body = Vec::new();
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .header("cookie", session.cookie_header())
            .body(Body::from(body))
            .unwrap();

        self.call(request).await
    }

    pub async fn export(&self, uri: &str, session: &Session) -> String {
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .header("cookie", session.cookie_header())
            .body(Body::empty())
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    async fn send_json(
        &self,
        method: &str,
        uri: &str,
        payload: Value,
        session: Option<&Session>,
    ) -> Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(session) = session {
            builder = builder.header("cookie", session.cookie_header());
        }
        self.call(builder.body(Body::from(payload.to_string())).unwrap())
            .await
    }

    async fn call(&self, request: Request<Body>) -> Response {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap();
        let body: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };

        Response {
            status,
            body,
            set_cookie,
        }
    }
}

impl Session {
    fn cookie_header(&self) -> String {
        self.cookie.split(';').next().unwrap_or_default().to_string()
    }
}
