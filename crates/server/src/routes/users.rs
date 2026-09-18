use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::auth::permission::AuditEntry;
use crate::auth::{password, session, CurrentUser};
use crate::db;
use crate::domain::user::{self, UserInput};
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::state::AppState;

const DEFAULT_RESET_PASSWORD: &str = "123456";
const MIN_PASSWORD_LEN: usize = 6;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users).post(create_user))
        .route("/users/options", get(list_options))
        .route("/users/{id}", put(update_user).delete(delete_user))
        .route("/users/{id}/password", post(reset_password))
        .route("/users/{id}/status", post(set_status))
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub keyword: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordInput {
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusInput {
    pub status: i64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOption {
    pub id: i64,
    pub display_name: String,
    pub dept: String,
}

async fn list_users(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<Vec<user::User>>> {
    current.require("user:manage")?;
    let keyword = query.keyword.trim().to_string();
    let users = db::run(state.pool.clone(), move |conn| user::list(conn, &keyword)).await?;
    Ok(Json(users))
}

/// 供「选择人员」下拉使用的精简列表，登录即可访问。
async fn list_options(
    State(state): State<AppState>,
    _current: CurrentUser,
) -> AppResult<Json<Vec<UserOption>>> {
    let options = db::run(state.pool.clone(), move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, display_name, dept FROM users WHERE status = 1 ORDER BY dept, display_name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UserOption {
                id: row.get(0)?,
                display_name: row.get(1)?,
                dept: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .await?;
    Ok(Json(options))
}

async fn create_user(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Json(input): Json<UserInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("user:manage")?;
    validate_username(&input.username)?;
    if input.display_name.trim().is_empty() {
        return Err(AppError::bad_request("请填写姓名"));
    }

    let plain = if input.password.trim().is_empty() {
        DEFAULT_RESET_PASSWORD.to_string()
    } else {
        input.password.clone()
    };
    if plain.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::bad_request(format!(
            "初始密码至少 {MIN_PASSWORD_LEN} 位"
        )));
    }

    let hash = password::hash_password(&plain)?;
    let username = input.username.clone();

    let user_id = db::run(state.pool.clone(), move |conn| {
        let id = user::create(conn, &input, &hash)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "新增用户",
            target_type: "user",
            target_id: &id.to_string(),
            detail: &username,
            ip: &ip,
        }
        .record(conn);
        Ok(id)
    })
    .await?;

    Ok(Json(json!({ "id": user_id, "initialPassword": plain })))
}

async fn update_user(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(user_id): Path<i64>,
    Json(input): Json<UserInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("user:manage")?;
    if input.display_name.trim().is_empty() {
        return Err(AppError::bad_request("请填写姓名"));
    }

    db::run(state.pool.clone(), move |conn| {
        user::update_profile(conn, user_id, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改用户",
            target_type: "user",
            target_id: &user_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn reset_password(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(user_id): Path<i64>,
    Json(input): Json<ResetPasswordInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("user:manage")?;

    let plain = if input.password.trim().is_empty() {
        DEFAULT_RESET_PASSWORD.to_string()
    } else {
        input.password.clone()
    };
    if plain.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::bad_request(format!(
            "密码至少 {MIN_PASSWORD_LEN} 位"
        )));
    }

    let hash = password::hash_password(&plain)?;
    db::run(state.pool.clone(), move |conn| {
        user::set_password(conn, user_id, &hash, true)?;
        session::revoke_all_for_user(conn, user_id)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "重置密码",
            target_type: "user",
            target_id: &user_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true, "password": plain })))
}

async fn set_status(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(user_id): Path<i64>,
    Json(input): Json<StatusInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("user:manage")?;
    if user_id == current.id {
        return Err(AppError::bad_request("不能停用自己的账号"));
    }
    if input.status != 0 && input.status != 1 {
        return Err(AppError::bad_request("状态值不合法"));
    }

    let status = input.status;
    db::run(state.pool.clone(), move |conn| {
        user::set_status(conn, user_id, status)?;
        if status == 0 {
            session::revoke_all_for_user(conn, user_id)?;
        }
        let action = if status == 1 { "启用用户" } else { "停用用户" };
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action,
            target_type: "user",
            target_id: &user_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_user(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(user_id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("user:manage")?;
    if user_id == current.id {
        return Err(AppError::bad_request("不能删除自己的账号"));
    }

    db::run(state.pool.clone(), move |conn| {
        user::delete(conn, user_id)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "删除用户",
            target_type: "user",
            target_id: &user_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

fn validate_username(username: &str) -> AppResult<()> {
    let username = username.trim();
    let len = username.chars().count();
    if !(2..=32).contains(&len) {
        return Err(AppError::bad_request("登录名长度需为 2-32 个字符"));
    }
    if username.chars().any(|c| c.is_whitespace()) {
        return Err(AppError::bad_request("登录名不能包含空格"));
    }
    Ok(())
}
