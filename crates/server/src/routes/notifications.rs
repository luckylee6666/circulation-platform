//! 站内消息与实时推送。

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::Sse;
use axum::response::sse::{Event, KeepAlive};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::auth::CurrentUser;
use crate::db;
use crate::domain::notify;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list).delete(clear))
        .route("/notifications/unread-count", get(unread_count))
        .route("/notifications/read", post(mark_read))
        .route("/events", get(events))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    unread_only: bool,
    limit: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    axum::extract::Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<notify::Notification>>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let user_id = current.id;

    let items = db::run(state.pool.clone(), move |conn| {
        notify::list(conn, user_id, query.unread_only, limit)
    })
    .await?;

    Ok(Json(items))
}

async fn unread_count(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = current.id;
    let count = db::run(state.pool.clone(), move |conn| {
        notify::unread_count(conn, user_id)
    })
    .await?;

    Ok(Json(serde_json::json!({ "count": count })))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ReadInput {
    /// 留空表示全部标记为已读
    #[serde(default)]
    ids: Vec<i64>,
}

async fn mark_read(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<ReadInput>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = current.id;
    let ids = input.ids;

    let affected = db::run(state.pool.clone(), move |conn| {
        notify::mark_read(conn, user_id, &ids)
    })
    .await?;

    Ok(Json(serde_json::json!({ "ok": true, "affected": affected })))
}

async fn clear(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = current.id;
    let affected = db::run(state.pool.clone(), move |conn| {
        Ok(conn.execute("DELETE FROM notifications WHERE user_id = ?1", [user_id])?)
    })
    .await?;

    Ok(Json(serde_json::json!({ "ok": true, "affected": affected })))
}

/// 服务端推送。
///
/// 只推「有变化了」，不带具体内容——前端收到后重新拉一次自己的数据。
/// 这样断线期间漏掉的推送不影响正确性，客户端靠兜底轮询也能补齐。
async fn events(
    State(state): State<AppState>,
    _current: CurrentUser,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.subscribe();

    let stream = BroadcastStream::new(receiver).filter_map(|result| {
        // 客户端跟不上导致积压时广播会返回 Lagged，跳过即可
        let signal = result.ok()?;
        let data = serde_json::to_string(&signal).ok()?;
        Some(Ok(Event::default().event("change").data(data)))
    });

    // 定期发注释行，让中间设备和浏览器都知道连接还活着
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}
