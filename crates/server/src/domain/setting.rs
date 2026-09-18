use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::util::now_str;

/// 平台名称的用户可见长度上限。
///
/// 侧边栏、登录页、浏览器标签页都要放得下，太长会把布局撑坏。
const SITE_NAME_MAX_CHARS: usize = 24;

pub const KEY_SITE_NAME: &str = "site_name";

/// 平台对外展示的名称，登录页也要用，所以不能依赖登录态。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteBranding {
    pub name: String,
}

pub fn get(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let value = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value)
}

pub fn get_or(conn: &Connection, key: &str, fallback: &str) -> AppResult<String> {
    let value = get(conn, key)?.unwrap_or_default();
    if value.trim().is_empty() {
        return Ok(fallback.to_string());
    }
    Ok(value)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now_str()],
    )?;
    Ok(())
}

pub fn branding(conn: &Connection) -> AppResult<SiteBranding> {
    Ok(SiteBranding {
        name: get_or(conn, KEY_SITE_NAME, DEFAULT_SITE_NAME)?,
    })
}

/// 校验并规范化用户填的平台名称。
pub fn normalize_site_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();

    if name.is_empty() {
        return Err(AppError::bad_request("平台名称不能为空"));
    }
    if name.chars().count() > SITE_NAME_MAX_CHARS {
        return Err(AppError::bad_request(format!(
            "平台名称不能超过 {SITE_NAME_MAX_CHARS} 个字"
        )));
    }
    // 换行会把侧边栏和标签页撑开，直接拒掉
    if name.contains(['\n', '\r', '\t']) {
        return Err(AppError::bad_request("平台名称不能包含换行或制表符"));
    }

    Ok(name.to_string())
}

pub const DEFAULT_SITE_NAME: &str = "流转平台";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_space() {
        assert_eq!(normalize_site_name("  某某办公平台  ").unwrap(), "某某办公平台");
    }

    #[test]
    fn rejects_blank_name() {
        assert!(normalize_site_name("   ").is_err());
    }

    #[test]
    fn counts_characters_not_bytes() {
        // 24 个汉字应当通过（按字符计数而不是字节）
        let ok = "一".repeat(SITE_NAME_MAX_CHARS);
        assert!(normalize_site_name(&ok).is_ok());

        let too_long = "一".repeat(SITE_NAME_MAX_CHARS + 1);
        assert!(normalize_site_name(&too_long).is_err());
    }

    #[test]
    fn rejects_line_breaks() {
        assert!(normalize_site_name("某某\n平台").is_err());
    }

    #[test]
    fn missing_key_falls_back_to_default() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);")
            .unwrap();

        assert_eq!(get_or(&conn, KEY_SITE_NAME, DEFAULT_SITE_NAME).unwrap(), DEFAULT_SITE_NAME);
    }

    #[test]
    fn set_then_get_round_trips() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);")
            .unwrap();

        set(&conn, KEY_SITE_NAME, "某某办公平台").unwrap();
        assert_eq!(get_or(&conn, KEY_SITE_NAME, DEFAULT_SITE_NAME).unwrap(), "某某办公平台");

        // 再写一次应当是更新而不是插入失败
        set(&conn, KEY_SITE_NAME, "另一个名字").unwrap();
        assert_eq!(get_or(&conn, KEY_SITE_NAME, DEFAULT_SITE_NAME).unwrap(), "另一个名字");
    }
}
