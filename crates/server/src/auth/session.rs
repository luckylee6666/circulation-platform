use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::auth::password::{generate_token, hash_token};
use crate::error::AppResult;
use crate::util::{now_plus_hours, now_str};

pub const SESSION_COOKIE: &str = "circulation_sid";

pub struct CreatedSession {
    pub token: String,
    pub expires_at: String,
}

/// 建立新会话，返回下发给浏览器的明文令牌。
pub fn create(
    conn: &Connection,
    user_id: i64,
    ip: &str,
    user_agent: &str,
    ttl_hours: i64,
) -> AppResult<CreatedSession> {
    let token = generate_token();
    let expires_at = now_plus_hours(ttl_hours);

    conn.execute(
        "INSERT INTO sessions (id, user_id, token_hash, ip, user_agent, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            Uuid::new_v4().to_string(),
            user_id,
            hash_token(&token),
            ip,
            truncate(user_agent, 255),
            now_str(),
            expires_at
        ],
    )?;

    Ok(CreatedSession { token, expires_at })
}

/// 用令牌换取用户 ID，过期或已吊销返回 None。
pub fn resolve_user(conn: &Connection, token: &str) -> AppResult<Option<i64>> {
    conn.query_row(
        "SELECT user_id FROM sessions
         WHERE token_hash = ?1 AND revoked_at IS NULL AND expires_at > ?2",
        params![hash_token(token), now_str()],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn revoke(conn: &Connection, token: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE sessions SET revoked_at = ?1 WHERE token_hash = ?2 AND revoked_at IS NULL",
        params![now_str(), hash_token(token)],
    )?;
    Ok(())
}

/// 吊销某个用户的全部会话（改密、停用、删除时调用）。
pub fn revoke_all_for_user(conn: &Connection, user_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE sessions SET revoked_at = ?1 WHERE user_id = ?2 AND revoked_at IS NULL",
        params![now_str(), user_id],
    )?;
    Ok(())
}

/// 清理过期会话记录，由定时任务调用。
pub fn purge_expired(conn: &Connection) -> AppResult<usize> {
    let removed = conn.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now_str()])?;
    Ok(removed)
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value.chars().take(max).collect()
}
