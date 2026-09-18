use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::auth::permission::AuditEntry;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::field::{self, FieldInput};
use crate::error::AppResult;
use crate::extract::ClientIp;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/fields", get(list_fields).post(create_field))
        .route("/fields/{id}", axum::routing::put(update_field).delete(delete_field))
}

/// 登录用户都要读字段定义（数据列表的列、导入的映射都依赖它）。
async fn list_fields(
    State(state): State<AppState>,
    _current: CurrentUser,
) -> AppResult<Json<Vec<field::FieldDef>>> {
    let fields = db::run(state.pool.clone(), move |conn| {
        field::list(conn, false)
    })
    .await?;
    Ok(Json(fields))
}

async fn create_field(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Json(input): Json<FieldInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("field:manage")?;

    let label = input.label.clone();
    let id = db::run(state.pool.clone(), move |conn| {
        let id = field::create(conn, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "新增字段",
            target_type: "field",
            target_id: &id.to_string(),
            detail: &label,
            ip: &ip,
        }
        .record(conn);
        Ok(id)
    })
    .await?;

    Ok(Json(json!({ "id": id })))
}

async fn update_field(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(input): Json<FieldInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("field:manage")?;

    db::run(state.pool.clone(), move |conn| {
        field::update(conn, id, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改字段",
            target_type: "field",
            target_id: &id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_field(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("field:manage")?;

    db::run(state.pool.clone(), move |conn| {
        field::delete(conn, id)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "删除字段",
            target_type: "field",
            target_id: &id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}
