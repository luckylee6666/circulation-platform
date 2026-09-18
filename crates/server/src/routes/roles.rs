use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::auth::permission::AuditEntry;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::role::{self, RoleInput};
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/roles", get(list_roles).post(create_role))
        .route("/roles/{id}", axum::routing::put(update_role).delete(delete_role))
        .route("/permissions", get(list_permissions))
}

async fn list_roles(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<Vec<role::Role>>> {
    require_role_read(&current)?;
    let roles = db::run(state.pool.clone(), move |conn| role::list(conn)).await?;
    Ok(Json(roles))
}

async fn list_permissions(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<Vec<role::Permission>>> {
    require_role_read(&current)?;
    let permissions = db::run(state.pool.clone(), move |conn| role::list_permissions(conn)).await?;
    Ok(Json(permissions))
}

async fn create_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Json(input): Json<RoleInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("role:manage")?;
    if input.name.trim().is_empty() {
        return Err(AppError::bad_request("请填写角色名称"));
    }

    let code = input.code.clone();
    let role_id = db::run(state.pool.clone(), move |conn| {
        let id = role::create(conn, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "新增角色",
            target_type: "role",
            target_id: &id.to_string(),
            detail: &code,
            ip: &ip,
        }
        .record(conn);
        Ok(id)
    })
    .await?;

    Ok(Json(json!({ "id": role_id })))
}

async fn update_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(role_id): Path<i64>,
    Json(input): Json<RoleInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("role:manage")?;
    if input.name.trim().is_empty() {
        return Err(AppError::bad_request("请填写角色名称"));
    }

    db::run(state.pool.clone(), move |conn| {
        role::update(conn, role_id, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改角色",
            target_type: "role",
            target_id: &role_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_role(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(role_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("role:manage")?;

    db::run(state.pool.clone(), move |conn| {
        role::delete(conn, role_id)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "删除角色",
            target_type: "role",
            target_id: &role_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// 用户管理页面也需要读取角色列表来分配角色，因此两种权限任一即可。
fn require_role_read(current: &CurrentUser) -> AppResult<()> {
    if current.has("role:manage") || current.has("user:manage") {
        return Ok(());
    }
    Err(AppError::forbidden("缺少「角色权限」或「用户管理」权限"))
}
