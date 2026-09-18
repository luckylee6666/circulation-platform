use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::{now_plus_minutes, now_str};

pub const MAX_FAILED_ATTEMPTS: i64 = 5;
pub const LOCK_MINUTES: i64 = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleBrief {
    pub id: i64,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub phone: String,
    pub dept: String,
    pub email: String,
    pub status: i64,
    pub must_change_password: bool,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub roles: Vec<RoleBrief>,
}

/// 登录校验需要的内部字段，不对外序列化。
pub struct AuthRecord {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub status: i64,
    pub must_change_password: bool,
    pub failed_attempts: i64,
    pub locked_until: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInput {
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub dept: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub role_ids: Vec<i64>,
}

pub fn find_auth_by_username(conn: &Connection, username: &str) -> AppResult<Option<AuthRecord>> {
    conn.query_row(
        "SELECT id, username, display_name, password_hash, status, must_change_password,
                failed_attempts, locked_until
         FROM users WHERE username = ?1",
        params![username],
        |row| {
            Ok(AuthRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                password_hash: row.get(3)?,
                status: row.get(4)?,
                must_change_password: row.get::<_, i64>(5)? != 0,
                failed_attempts: row.get(6)?,
                locked_until: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn find_password_hash(conn: &Connection, user_id: i64) -> AppResult<String> {
    conn.query_row("SELECT password_hash FROM users WHERE id = ?1", params![user_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("用户不存在"))
}

pub fn find_by_id(conn: &Connection, user_id: i64) -> AppResult<Option<User>> {
    let user = conn
        .query_row(
            "SELECT id, username, display_name, phone, dept, email, status,
                    must_change_password, last_login_at, created_at
             FROM users WHERE id = ?1",
            params![user_id],
            map_user_row,
        )
        .optional()?;

    match user {
        Some(mut user) => {
            user.roles = roles_of(conn, user.id)?;
            Ok(Some(user))
        }
        None => Ok(None),
    }
}

pub fn list(conn: &Connection, keyword: &str) -> AppResult<Vec<User>> {
    let mut stmt = conn.prepare(
        "SELECT id, username, display_name, phone, dept, email, status,
                must_change_password, last_login_at, created_at
         FROM users
         WHERE (?1 = '' OR username LIKE ?2 OR display_name LIKE ?2 OR dept LIKE ?2)
         ORDER BY id ASC",
    )?;

    let pattern = format!("%{keyword}%");
    let rows = stmt.query_map(params![keyword, pattern], map_user_row)?;
    let mut users: Vec<User> = rows.collect::<Result<_, _>>()?;

    let mut role_map = roles_grouped(conn)?;
    for user in &mut users {
        user.roles = role_map.remove(&user.id).unwrap_or_default();
    }
    Ok(users)
}

pub fn create(conn: &Connection, input: &UserInput, password_hash: &str) -> AppResult<i64> {
    let now = now_str();
    conn.execute(
        "INSERT INTO users (username, display_name, password_hash, phone, dept, email,
                            status, must_change_password, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, ?7, ?7)",
        params![
            input.username.trim(),
            input.display_name.trim(),
            password_hash,
            input.phone.trim(),
            input.dept.trim(),
            input.email.trim(),
            now
        ],
    )?;
    let user_id = conn.last_insert_rowid();
    set_roles(conn, user_id, &input.role_ids)?;
    Ok(user_id)
}

pub fn update_profile(conn: &Connection, user_id: i64, input: &UserInput) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE users SET display_name = ?1, phone = ?2, dept = ?3, email = ?4, updated_at = ?5
         WHERE id = ?6",
        params![
            input.display_name.trim(),
            input.phone.trim(),
            input.dept.trim(),
            input.email.trim(),
            now_str(),
            user_id
        ],
    )?;
    if affected == 0 {
        return Err(AppError::not_found("用户不存在"));
    }
    set_roles(conn, user_id, &input.role_ids)?;
    Ok(())
}

pub fn set_password(conn: &Connection, user_id: i64, password_hash: &str, must_change: bool) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE users SET password_hash = ?1, must_change_password = ?2, updated_at = ?3,
                          failed_attempts = 0, locked_until = NULL
         WHERE id = ?4",
        params![password_hash, i64::from(must_change), now_str(), user_id],
    )?;
    if affected == 0 {
        return Err(AppError::not_found("用户不存在"));
    }
    Ok(())
}

pub fn set_status(conn: &Connection, user_id: i64, status: i64) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE users SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now_str(), user_id],
    )?;
    if affected == 0 {
        return Err(AppError::not_found("用户不存在"));
    }
    Ok(())
}

pub fn delete(conn: &Connection, user_id: i64) -> AppResult<()> {
    let affected = conn.execute("DELETE FROM users WHERE id = ?1", params![user_id])?;
    if affected == 0 {
        return Err(AppError::not_found("用户不存在"));
    }
    Ok(())
}

pub fn set_roles(conn: &Connection, user_id: i64, role_ids: &[i64]) -> AppResult<()> {
    conn.execute("DELETE FROM user_roles WHERE user_id = ?1", params![user_id])?;
    for role_id in role_ids {
        conn.execute(
            "INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?1, ?2)",
            params![user_id, role_id],
        )?;
    }
    Ok(())
}

pub fn is_admin(conn: &Connection, user_id: i64) -> AppResult<bool> {
    let found = conn
        .query_row(
            "SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id
             WHERE ur.user_id = ?1 AND r.code = 'admin'",
            params![user_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(found)
}

pub fn count_by_role(conn: &Connection, role_id: i64) -> AppResult<i64> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM user_roles WHERE role_id = ?1",
        params![role_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

// ---------- 登录失败计数 ----------

pub fn record_login_failure(conn: &Connection, user_id: i64, attempts: i64) -> AppResult<()> {
    let next = attempts + 1;
    let locked_until: Option<String> = if next >= MAX_FAILED_ATTEMPTS {
        Some(now_plus_minutes(LOCK_MINUTES))
    } else {
        None
    };

    conn.execute(
        "UPDATE users SET failed_attempts = ?1, locked_until = ?2, updated_at = ?3 WHERE id = ?4",
        params![next, locked_until, now_str(), user_id],
    )?;
    Ok(())
}

pub fn mark_login_success(conn: &Connection, user_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE users SET failed_attempts = 0, locked_until = NULL, last_login_at = ?1, updated_at = ?1
         WHERE id = ?2",
        params![now_str(), user_id],
    )?;
    Ok(())
}

// ---------- 内部辅助 ----------

fn map_user_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        phone: row.get(3)?,
        dept: row.get(4)?,
        email: row.get(5)?,
        status: row.get(6)?,
        must_change_password: row.get::<_, i64>(7)? != 0,
        last_login_at: row.get(8)?,
        created_at: row.get(9)?,
        roles: Vec::new(),
    })
}

fn roles_of(conn: &Connection, user_id: i64) -> AppResult<Vec<RoleBrief>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.code, r.name FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = ?1 ORDER BY r.id",
    )?;
    let rows = stmt.query_map(params![user_id], |row| {
        Ok(RoleBrief {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn roles_grouped(conn: &Connection) -> AppResult<std::collections::HashMap<i64, Vec<RoleBrief>>> {
    let mut stmt = conn.prepare(
        "SELECT ur.user_id, r.id, r.code, r.name FROM user_roles ur
         JOIN roles r ON r.id = ur.role_id
         ORDER BY r.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            RoleBrief {
                id: row.get(1)?,
                code: row.get(2)?,
                name: row.get(3)?,
            },
        ))
    })?;

    let mut map: std::collections::HashMap<i64, Vec<RoleBrief>> = std::collections::HashMap::new();
    for row in rows {
        let (user_id, role) = row?;
        map.entry(user_id).or_default().push(role);
    }
    Ok(map)
}
