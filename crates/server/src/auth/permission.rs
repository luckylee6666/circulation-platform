use std::collections::HashSet;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use rusqlite::{params, Connection, OptionalExtension};

use crate::auth::session::{self, SESSION_COOKIE};
use crate::db;
use crate::domain::{role, user};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// 当前登录用户。作为提取器使用时，未登录会直接返回 401。
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub must_change_password: bool,
    pub roles: Vec<String>,
    pub permissions: HashSet<String>,
}

impl CurrentUser {
    pub fn require(&self, permission: &str) -> AppResult<()> {
        if self.permissions.contains(permission) {
            Ok(())
        } else {
            Err(AppError::forbidden(format!("缺少「{permission}」权限")))
        }
    }

    pub fn has(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|code| code == role::ADMIN_ROLE_CODE)
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|cookie| cookie.value().to_string())
            .ok_or_else(|| AppError::unauthorized("请先登录"))?;

        let pool = state.pool.clone();
        db::run(pool, move |conn| load(conn, &token))
            .await?
            .ok_or_else(|| AppError::unauthorized("登录状态已失效，请重新登录"))
    }
}

fn load(conn: &Connection, token: &str) -> AppResult<Option<CurrentUser>> {
    let Some(user_id) = session::resolve_user(conn, token)? else {
        return Ok(None);
    };

    let record = conn
        .query_row(
            "SELECT username, display_name, status, must_change_password FROM users WHERE id = ?1",
            params![user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;

    let Some((username, display_name, status, must_change)) = record else {
        return Ok(None);
    };

    if status != 1 {
        return Ok(None);
    }

    Ok(Some(CurrentUser {
        id: user_id,
        username,
        display_name,
        must_change_password: must_change != 0,
        roles: role::role_codes_of_user(conn, user_id)?,
        permissions: role::permissions_of_user(conn, user_id)?,
    }))
}

/// 一条审计记录。用结构体而不是位置参数，避免连续几个字符串参数写错顺序。
pub struct AuditEntry<'a> {
    pub actor_id: Option<i64>,
    pub actor_name: &'a str,
    pub action: &'a str,
    pub target_type: &'a str,
    pub target_id: &'a str,
    pub detail: &'a str,
    pub ip: &'a str,
}

impl AuditEntry<'_> {
    /// 写审计日志。失败不影响主流程。
    pub fn record(&self, conn: &Connection) {
        let result = conn.execute(
            "INSERT INTO audit_logs (actor_id, actor_name, action, target_type, target_id, detail, ip, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                self.actor_id,
                self.actor_name,
                self.action,
                self.target_type,
                self.target_id,
                self.detail,
                self.ip,
                crate::util::now_str()
            ],
        );

        if let Err(err) = result {
            tracing::warn!("写入审计日志失败: {err}");
        }
    }
}

/// 供业务层复用的用户存在性校验。
pub fn ensure_user_exists(conn: &Connection, user_id: i64) -> AppResult<()> {
    if user::find_by_id(conn, user_id)?.is_none() {
        return Err(AppError::not_found("用户不存在"));
    }
    Ok(())
}
