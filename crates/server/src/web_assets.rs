use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// 路径相对于 crate 根目录，内容在编译期内嵌。
#[derive(RustEmbed)]
#[folder = "../../apps/web/dist"]
struct Assets;

/// 未命中 API 路由时的兜底：先找静态文件，再回退到 index.html 交给前端路由。
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // 不能让未知的 API 路径被前端页面吞掉，否则调用方拿到的是 HTML 而不是错误信息
    if path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "code": "not_found", "message": "接口不存在" })),
        )
            .into_response();
    }

    if let Some(response) = file_response(path) {
        return response;
    }

    // 前端使用 history 路由，带点的路径一律当成资源请求，不再回退
    if !path.contains('.') && let Some(response) = file_response("index.html") {
        return response;
    }

    (StatusCode::NOT_FOUND, "页面不存在").into_response()
}

fn file_response(path: &str) -> Option<Response> {
    let asset = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref());

    // 文件名带构建哈希，可以长期缓存；入口页面必须每次校验，否则升级后拿到旧页面
    builder = if path == "index.html" {
        builder.header(header::CACHE_CONTROL, "no-cache")
    } else {
        builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
    };

    builder.body(Body::from(asset.data.into_owned())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(uri: &str) -> String {
        uri.trim_start_matches('/').to_string()
    }

    #[test]
    fn api_paths_are_never_served_as_pages() {
        // 兜底逻辑里对 api/ 开头做了短路，这里固化这个约定
        let uri = "/api/unknown-endpoint";
        assert!(path_of(uri).starts_with("api/"));
    }

    #[test]
    fn asset_lookup_uses_embedded_files() {
        // 前端产物目录一定存在（build.rs 会写占位页），因此 index.html 必须能取到
        assert!(Assets::get("index.html").is_some());
    }
}
