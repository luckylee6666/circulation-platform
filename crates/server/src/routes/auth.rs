use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::auth::{password, session, CurrentUser};
use crate::db;
use crate::domain::{role, user};
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::state::AppState;
use crate::util::now_str;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/change-password", post(change_password))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub user: user::User,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub must_change_password: bool,
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(input): Json<LoginInput>,
) -> AppResult<(CookieJar, Json<SessionInfo>)> {
    let username = input.username.trim().to_string();
    if username.is_empty() || input.password.is_empty() {
        return Err(AppError::bad_request("请输入用户名和密码"));
    }

    let password = input.password;
    let user_agent = headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let ttl = state.config.session_ttl_hours;

    let (token, user_id) = db::run(state.pool.clone(), move |conn| {
        let Some(record) = user::find_auth_by_username(conn, &username)? else {
            return Err(AppError::unauthorized("用户名或密码错误"));
        };

        if let Some(locked_until) = record.locked_until.as_deref()
            && locked_until > now_str().as_str()
        {
            return Err(AppError::forbidden(format!(
                "账号已锁定，请于 {locked_until} 之后重试"
            )));
        }

        if !password::verify_password(&password, &record.password_hash) {
            user::record_login_failure(conn, record.id, record.failed_attempts)?;
            tracing::warn!(username = %record.username, ip = %ip, "登录失败：密码错误");
            return Err(AppError::unauthorized("用户名或密码错误"));
        }

        if record.status != 1 {
            return Err(AppError::forbidden("账号已停用，请联系管理员"));
        }

        user::mark_login_success(conn, record.id)?;
        let created = session::create(conn, record.id, &ip, &user_agent, ttl)?;
        tracing::info!(username = %record.username, ip = %ip, "登录成功");
        Ok((created.token, record.id))
    })
    .await?;

    let info = load_session_info(&state, user_id).await?;
    Ok((jar.add(session_cookie(token, ttl)), Json(info)))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<serde_json::Value>)> {
    if let Some(token) = jar.get(session::SESSION_COOKIE).map(|c| c.value().to_string()) {
        db::run(state.pool.clone(), move |conn| session::revoke(conn, &token)).await?;
    }
    Ok((jar.remove(remove_cookie()), Json(serde_json::json!({ "ok": true }))))
}

async fn me(State(state): State<AppState>, current: CurrentUser) -> AppResult<Json<SessionInfo>> {
    Ok(Json(load_session_info(&state, current.id).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordInput {
    pub old_password: String,
    pub new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    jar: CookieJar,
    ClientIp(ip): ClientIp,
    current: CurrentUser,
    Json(input): Json<ChangePasswordInput>,
) -> AppResult<(CookieJar, Json<serde_json::Value>)> {
    let new_password = input.new_password;
    if new_password.chars().count() < 6 {
        return Err(AppError::bad_request("新密码至少 6 位"));
    }
    if new_password == input.old_password {
        return Err(AppError::bad_request("新密码不能与当前密码相同"));
    }

    let user_id = current.id;
    let old_password = input.old_password;
    let new_hash = password::hash_password(&new_password)?;
    let ttl = state.config.session_ttl_hours;

    let token = db::run(state.pool.clone(), move |conn| {
        let stored = user::find_password_hash(conn, user_id)?;
        if !password::verify_password(&old_password, &stored) {
            return Err(AppError::bad_request("当前密码不正确"));
        }

        user::set_password(conn, user_id, &new_hash, false)?;
        // 改密后让其它设备上的登录立即失效
        session::revoke_all_for_user(conn, user_id)?;
        let created = session::create(conn, user_id, &ip, "", ttl)?;
        crate::auth::permission::AuditEntry {
            actor_id: Some(user_id),
            actor_name: &current.display_name,
            action: "修改密码",
            target_type: "user",
            target_id: &user_id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(created.token)
    })
    .await?;

    Ok((
        jar.add(session_cookie(token, ttl)),
        Json(serde_json::json!({ "ok": true })),
    ))
}

async fn load_session_info(state: &AppState, user_id: i64) -> AppResult<SessionInfo> {
    db::run(state.pool.clone(), move |conn| {
        let user = user::find_by_id(conn, user_id)?.ok_or_else(|| AppError::not_found("用户不存在"))?;
        let roles = role::role_codes_of_user(conn, user_id)?;

        let mut permissions: Vec<String> =
            role::permissions_of_user(conn, user_id)?.into_iter().collect();
        permissions.sort();

        Ok(SessionInfo {
            must_change_password: user.must_change_password,
            user,
            roles,
            permissions,
        })
    })
    .await
}

fn session_cookie(token: String, ttl_hours: i64) -> Cookie<'static> {
    Cookie::build((session::SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::hours(ttl_hours))
        .build()
}

fn remove_cookie() -> Cookie<'static> {
    Cookie::build((session::SESSION_COOKIE, ""))
        .path("/")
        .build()
}
