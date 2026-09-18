use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::now_str;

pub const ADMIN_ROLE_CODE: &str = "admin";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub is_system: bool,
    pub user_count: i64,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub code: String,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleInput {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

pub fn list(conn: &Connection) -> AppResult<Vec<Role>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.code, r.name, r.description, r.is_system,
                (SELECT COUNT(*) FROM user_roles ur WHERE ur.role_id = r.id)
         FROM roles r ORDER BY r.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Role {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            is_system: row.get::<_, i64>(4)? != 0,
            user_count: row.get(5)?,
            permissions: Vec::new(),
        })
    })?;

    let mut roles: Vec<Role> = rows.collect::<Result<_, _>>()?;
    let mut permission_map = permissions_grouped(conn)?;
    for role in &mut roles {
        role.permissions = permission_map.remove(&role.id).unwrap_or_default();
    }
    Ok(roles)
}

pub fn find_by_id(conn: &Connection, role_id: i64) -> AppResult<Option<Role>> {
    let mut roles = list(conn)?;
    Ok(roles.drain(..).find(|role| role.id == role_id))
}

pub fn find_by_code(conn: &Connection, code: &str) -> AppResult<Option<Role>> {
    let id = conn
        .query_row("SELECT id FROM roles WHERE code = ?1", params![code], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?;
    match id {
        Some(id) => find_by_id(conn, id),
        None => Ok(None),
    }
}

pub fn create(conn: &Connection, input: &RoleInput) -> AppResult<i64> {
    let code = input.code.trim();
    if code.is_empty() {
        return Err(AppError::bad_request("角色标识不能为空"));
    }

    conn.execute(
        "INSERT INTO roles (code, name, description, is_system, created_at) VALUES (?1, ?2, ?3, 0, ?4)",
        params![code, input.name.trim(), input.description.trim(), now_str()],
    )?;
    let role_id = conn.last_insert_rowid();
    set_permissions(conn, role_id, &input.permissions)?;
    Ok(role_id)
}

pub fn update(conn: &Connection, role_id: i64, input: &RoleInput) -> AppResult<()> {
    let is_system = conn
        .query_row("SELECT is_system FROM roles WHERE id = ?1", params![role_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("角色不存在"))?
        != 0;

    // 系统角色的标识不允许改，避免内置流程里的角色引用失效
    if is_system {
        conn.execute(
            "UPDATE roles SET name = ?1, description = ?2 WHERE id = ?3",
            params![input.name.trim(), input.description.trim(), role_id],
        )?;
    } else {
        conn.execute(
            "UPDATE roles SET code = ?1, name = ?2, description = ?3 WHERE id = ?4",
            params![input.code.trim(), input.name.trim(), input.description.trim(), role_id],
        )?;
    }

    set_permissions(conn, role_id, &input.permissions)?;
    Ok(())
}

pub fn delete(conn: &Connection, role_id: i64) -> AppResult<()> {
    let is_system = conn
        .query_row("SELECT is_system FROM roles WHERE id = ?1", params![role_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("角色不存在"))?
        != 0;

    if is_system {
        return Err(AppError::bad_request("内置角色不允许删除"));
    }

    let user_count = crate::domain::user::count_by_role(conn, role_id)?;
    if user_count > 0 {
        return Err(AppError::bad_request(format!(
            "还有 {user_count} 个用户属于该角色，请先调整用户角色"
        )));
    }

    conn.execute("DELETE FROM roles WHERE id = ?1", params![role_id])?;
    Ok(())
}

pub fn set_permissions(conn: &Connection, role_id: i64, codes: &[String]) -> AppResult<()> {
    conn.execute("DELETE FROM role_permissions WHERE role_id = ?1", params![role_id])?;
    for code in codes {
        conn.execute(
            "INSERT OR IGNORE INTO role_permissions (role_id, permission_code) VALUES (?1, ?2)",
            params![role_id, code],
        )?;
    }
    Ok(())
}

/// 用户实际拥有的权限集合。管理员角色始终拥有全部权限，防止误配把自己锁在门外。
pub fn permissions_of_user(conn: &Connection, user_id: i64) -> AppResult<HashSet<String>> {
    if crate::domain::user::is_admin(conn, user_id)? {
        return all_permission_codes(conn);
    }

    let mut stmt = conn.prepare(
        "SELECT DISTINCT rp.permission_code
         FROM role_permissions rp
         JOIN user_roles ur ON ur.role_id = rp.role_id
         WHERE ur.user_id = ?1",
    )?;
    let rows = stmt.query_map(params![user_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

pub fn role_codes_of_user(conn: &Connection, user_id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT r.code FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         WHERE ur.user_id = ?1 ORDER BY r.id",
    )?;
    let rows = stmt.query_map(params![user_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn list_permissions(conn: &Connection) -> AppResult<Vec<Permission>> {
    let mut stmt =
        conn.prepare("SELECT code, name, category FROM permissions ORDER BY category, sort, code")?;
    let rows = stmt.query_map([], |row| {
        Ok(Permission {
            code: row.get(0)?,
            name: row.get(1)?,
            category: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn all_permission_codes(conn: &Connection) -> AppResult<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT code FROM permissions")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<HashSet<_>, _>>()?)
}

fn permissions_grouped(conn: &Connection) -> AppResult<std::collections::HashMap<i64, Vec<String>>> {
    let mut stmt =
        conn.prepare("SELECT role_id, permission_code FROM role_permissions ORDER BY permission_code")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut map: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for row in rows {
        let (role_id, code) = row?;
        map.entry(role_id).or_default().push(code);
    }
    Ok(map)
}
