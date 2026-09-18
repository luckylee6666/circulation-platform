use axum::extract::ConnectInfo;
use axum::http::request::Parts;
use axum::extract::FromRequestParts;
use std::convert::Infallible;
use std::net::SocketAddr;

/// 客户端 IP。依赖 `ConnectInfo`，取不到时回退到代理头，再取不到就是空串。
/// 永远不失败，避免因为缺少连接信息导致请求直接 500。
pub struct ClientIp(pub String);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(ConnectInfo(addr)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
            return Ok(ClientIp(addr.ip().to_string()));
        }

        if let Some(forwarded) = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            let first = forwarded.split(',').next().unwrap_or_default().trim();
            if !first.is_empty() {
                return Ok(ClientIp(first.to_string()));
            }
        }

        if let Some(real_ip) = parts.headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            return Ok(ClientIp(real_ip.trim().to_string()));
        }

        Ok(ClientIp(String::new()))
    }
}
