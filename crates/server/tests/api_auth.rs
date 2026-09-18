//! 认证与权限相关的端到端测试。脚手架见 common/mod.rs。

mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::TestApp;
// ---------- 测试 ----------

#[tokio::test]
async fn admin_can_update_profile_and_roles() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let roles = app.get("/api/roles", Some(&session)).await;
    let find_role = |code: &str| {
        roles.body.as_array().unwrap().iter()
            .find(|role| role["code"] == code)
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    let viewer_role = find_role("viewer");
    let handler_role = find_role("handler");

    let created = app
        .post(
            "/api/users",
            json!({
                "username": "wangwu",
                "displayName": "王五",
                "dept": "业务二科",
                "roleIds": [viewer_role]
            }),
            Some(&session),
        )
        .await;
    let user_id = created.body["id"].as_i64().unwrap();

    // 只读角色不能访问用户管理
    let viewer_session = app.login("wangwu", "123456").await.unwrap();
    assert_eq!(
        app.get("/api/users", Some(&viewer_session)).await.status,
        StatusCode::FORBIDDEN
    );

    // 改资料 + 换角色
    let updated = app
        .put(
            &format!("/api/users/{user_id}"),
            json!({
                "username": "wangwu",
                "displayName": "王五五",
                "dept": "业务三科",
                "phone": "13800000000",
                "roleIds": [handler_role]
            }),
            Some(&session),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK, "更新用户失败: {:?}", updated.body);

    let relogin = app.login("wangwu", "123456").await.unwrap();
    let me = app.get("/api/auth/me", Some(&relogin)).await;
    assert_eq!(me.body["user"]["displayName"], "王五五");
    assert_eq!(me.body["user"]["dept"], "业务三科");
    assert_eq!(me.body["roles"][0], "handler");
}

#[tokio::test]
async fn user_options_are_visible_to_any_logged_in_user() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let created = app
        .post(
            "/api/users",
            json!({ "username": "zhaoliu", "displayName": "赵六", "dept": "业务一科" }),
            Some(&session),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK);

    let options = app.get("/api/users/options", Some(&session)).await;
    assert_eq!(options.status, StatusCode::OK);
    let list = options.body.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|item| item["displayName"] == "赵六"));
    // 不应暴露用户名等敏感字段
    assert!(list[0].get("username").is_none());

    assert_eq!(
        app.get("/api/users/options", None).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn health_endpoint_is_open() {
    let app = TestApp::new();
    let response = app.get("/api/health", None).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["status"], "ok");
}

#[tokio::test]
async fn protected_endpoints_reject_anonymous() {
    let app = TestApp::new();

    for uri in ["/api/auth/me", "/api/users", "/api/roles", "/api/permissions"] {
        let response = app.get(uri, None).await;
        assert_eq!(
            response.status,
            StatusCode::UNAUTHORIZED,
            "{uri} 应该拒绝未登录请求"
        );
    }
}

#[tokio::test]
async fn admin_can_login_and_read_own_profile() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.expect("管理员登录失败");

    let response = app.get("/api/auth/me", Some(&session)).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["user"]["username"], "admin");
    assert_eq!(response.body["user"]["displayName"], "系统管理员");
    assert_eq!(response.body["mustChangePassword"], true);
    assert_eq!(response.body["roles"][0], "admin");

    let permissions = response.body["permissions"].as_array().unwrap();
    assert!(
        permissions.iter().any(|p| p == "user:manage"),
        "管理员应拥有 user:manage"
    );
}

#[tokio::test]
async fn wrong_password_is_rejected_and_counted() {
    let app = TestApp::new();

    let response = app
        .post(
            "/api/auth/login",
            json!({ "username": "admin", "password": "wrong-password" }),
            None,
        )
        .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.body["code"], "unauthorized");
    // 错误信息不应泄露账号是否存在
    assert_eq!(response.body["message"], "用户名或密码错误");
}

#[tokio::test]
async fn unknown_user_gets_same_error_as_wrong_password() {
    let app = TestApp::new();

    let response = app
        .post(
            "/api/auth/login",
            json!({ "username": "nobody", "password": "whatever" }),
            None,
        )
        .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.body["message"], "用户名或密码错误");
}

#[tokio::test]
async fn account_locks_after_repeated_failures() {
    let app = TestApp::new();

    for _ in 0..5 {
        let response = app
            .post(
                "/api/auth/login",
                json!({ "username": "admin", "password": "nope" }),
                None,
            )
            .await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    }

    // 第 6 次即使密码正确也应被锁定
    let response = app
        .post(
            "/api/auth/login",
            json!({ "username": "admin", "password": "admin123" }),
            None,
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert!(
        response.body["message"].as_str().unwrap().contains("锁定"),
        "应提示账号已锁定，实际: {}",
        response.body["message"]
    );
}

#[tokio::test]
async fn logout_invalidates_session() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    assert_eq!(app.get("/api/auth/me", Some(&session)).await.status, StatusCode::OK);

    let response = app.post("/api/auth/logout", json!({}), Some(&session)).await;
    assert_eq!(response.status, StatusCode::OK);

    assert_eq!(
        app.get("/api/auth/me", Some(&session)).await.status,
        StatusCode::UNAUTHORIZED,
        "登出后会话应失效"
    );
}

#[tokio::test]
async fn admin_can_manage_users() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    // 初始只有管理员
    let users = app.get("/api/users", Some(&session)).await;
    assert_eq!(users.status, StatusCode::OK);
    assert_eq!(users.body.as_array().unwrap().len(), 1);

    // 新建用户并分配「承办人」角色
    let roles = app.get("/api/roles", Some(&session)).await;
    let handler_role = roles.body.as_array().unwrap().iter()
        .find(|role| role["code"] == "handler")
        .expect("缺少承办人角色")["id"]
        .as_i64()
        .unwrap();

    let created = app
        .post(
            "/api/users",
            json!({
                "username": "zhangsan",
                "displayName": "张三",
                "password": "zhangsan123",
                "dept": "业务一科",
                "roleIds": [handler_role]
            }),
            Some(&session),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "创建用户失败: {:?}", created.body);
    let user_id = created.body["id"].as_i64().unwrap();

    // 新用户能登录，且首登被要求改密
    let new_session = app.login("zhangsan", "zhangsan123").await.expect("新用户登录失败");
    let me = app.get("/api/auth/me", Some(&new_session)).await;
    assert_eq!(me.body["user"]["displayName"], "张三");
    assert_eq!(me.body["mustChangePassword"], true);

    // 但新用户没有用户管理权限
    let forbidden = app.get("/api/users", Some(&new_session)).await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    let forbidden = app
        .post("/api/users", json!({"username": "x", "displayName": "y"}), Some(&new_session))
        .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    // 停用后无法登录
    let disabled = app
        .post(&format!("/api/users/{user_id}/status"), json!({ "status": 0 }), Some(&session))
        .await;
    assert_eq!(disabled.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/auth/me", Some(&new_session)).await.status,
        StatusCode::UNAUTHORIZED,
        "停用后已有会话应立即失效"
    );
    let relogin = app.login("zhangsan", "zhangsan123").await;
    assert!(relogin.is_err(), "停用账号不应能登录");
}

#[tokio::test]
async fn admin_cannot_delete_or_disable_self() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let me = app.get("/api/auth/me", Some(&session)).await;
    let admin_id = me.body["user"]["id"].as_i64().unwrap();

    let disable = app
        .post(&format!("/api/users/{admin_id}/status"), json!({ "status": 0 }), Some(&session))
        .await;
    assert_eq!(disable.status, StatusCode::BAD_REQUEST);

    let delete = app.delete(&format!("/api/users/{admin_id}"), &session).await;
    assert_eq!(delete.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_username_is_rejected() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let first = app
        .post(
            "/api/users",
            json!({ "username": "lisi", "displayName": "李四" }),
            Some(&session),
        )
        .await;
    assert_eq!(first.status, StatusCode::OK);
    // 未填密码时应下发默认初始密码，并强制首登修改
    assert_eq!(first.body["initialPassword"], "123456");

    let second = app
        .post(
            "/api/users",
            json!({ "username": "lisi", "displayName": "李四二号" }),
            Some(&session),
        )
        .await;
    assert_eq!(second.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn change_password_requires_correct_old_password() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let wrong = app
        .post(
            "/api/auth/change-password",
            json!({ "oldPassword": "bad", "newPassword": "newpass123" }),
            Some(&session),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);

    let too_short = app
        .post(
            "/api/auth/change-password",
            json!({ "oldPassword": "admin123", "newPassword": "123" }),
            Some(&session),
        )
        .await;
    assert_eq!(too_short.status, StatusCode::BAD_REQUEST);

    let ok = app
        .post(
            "/api/auth/change-password",
            json!({ "oldPassword": "admin123", "newPassword": "newpass123" }),
            Some(&session),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "改密失败: {:?}", ok.body);

    // 旧会话应失效，新密码可登录
    assert_eq!(
        app.get("/api/auth/me", Some(&session)).await.status,
        StatusCode::UNAUTHORIZED,
        "改密后旧会话应失效"
    );

    let fresh = app.login("admin", "newpass123").await.expect("新密码登录失败");
    let me = app.get("/api/auth/me", Some(&fresh)).await;
    assert_eq!(me.body["mustChangePassword"], false);
}

#[tokio::test]
async fn role_list_exposes_permissions() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let roles = app.get("/api/roles", Some(&session)).await;
    assert_eq!(roles.status, StatusCode::OK);

    let all = roles.body.as_array().unwrap();
    assert_eq!(all.len(), 5, "应有 5 个内置角色");

    let viewer = all.iter().find(|role| role["code"] == "viewer").unwrap();
    let permissions = viewer["permissions"].as_array().unwrap();
    assert!(permissions.iter().any(|p| p == "data:view"));
    assert!(!permissions.iter().any(|p| p == "user:manage"));
    assert_eq!(viewer["isSystem"], true);
}

#[tokio::test]
async fn system_role_cannot_be_deleted() {
    let app = TestApp::new();
    let session = app.login("admin", "admin123").await.unwrap();

    let roles = app.get("/api/roles", Some(&session)).await;
    let admin_role = roles.body.as_array().unwrap().iter()
        .find(|role| role["code"] == "admin")
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    let response = app.delete(&format!("/api/roles/{admin_role}"), &session).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}
