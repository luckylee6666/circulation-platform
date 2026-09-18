use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::util::now_str;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordItem {
    pub id: i64,
    pub ext_key: String,
    pub data: Map<String, Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPage {
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<RecordItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatch {
    pub id: i64,
    pub filename: String,
    pub total_rows: i64,
    pub inserted: i64,
    pub updated: i64,
    pub skipped: i64,
    pub failed: i64,
    pub operator_id: Option<i64>,
    pub operator_name: String,
    pub created_at: String,
}

/// 只匹配记录的「值」，不匹配字段名，否则搜「编号」会把所有记录都搜出来。
const SEARCH_CLAUSE: &str = "EXISTS (SELECT 1 FROM json_each(r.data) WHERE CAST(json_each.value AS TEXT) LIKE ?2)";

pub fn list(conn: &Connection, keyword: &str, page: i64, page_size: i64) -> AppResult<RecordPage> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 200);
    let keyword = keyword.trim().to_string();
    let pattern = format!("%{keyword}%");

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM records r WHERE ?1 = '' OR {SEARCH_CLAUSE}"),
        params![keyword, pattern],
        |row| row.get(0),
    )?;

    let sql = format!(
        "SELECT r.id, r.ext_key, r.data, r.created_at, r.updated_at
         FROM records r
         WHERE ?1 = '' OR {SEARCH_CLAUSE}
         ORDER BY r.id DESC
         LIMIT ?3 OFFSET ?4"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![keyword, pattern, page_size, (page - 1) * page_size],
        |row| {
            let data: String = row.get(2)?;
            Ok(RecordItem {
                id: row.get(0)?,
                ext_key: row.get(1)?,
                data: serde_json::from_str(&data).unwrap_or_default(),
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )?;

    Ok(RecordPage {
        total,
        page,
        page_size,
        items: rows.collect::<Result<Vec<_>, _>>()?,
    })
}

/// 导出用：一次取出全部记录（上限保护由调用方决定）。
pub fn list_all(conn: &Connection, keyword: &str, limit: i64) -> AppResult<Vec<RecordItem>> {
    let keyword = keyword.trim().to_string();
    let pattern = format!("%{keyword}%");

    let sql = format!(
        "SELECT r.id, r.ext_key, r.data, r.created_at, r.updated_at
         FROM records r
         WHERE ?1 = '' OR {SEARCH_CLAUSE}
         ORDER BY r.id DESC
         LIMIT ?3"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![keyword, pattern, limit], |row| {
        let data: String = row.get(2)?;
        Ok(RecordItem {
            id: row.get(0)?,
            ext_key: row.get(1)?,
            data: serde_json::from_str(&data).unwrap_or_default(),
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn find(conn: &Connection, id: i64) -> AppResult<Option<RecordItem>> {
    conn.query_row(
        "SELECT id, ext_key, data, created_at, updated_at FROM records WHERE id = ?1",
        params![id],
        |row| {
            let data: String = row.get(2)?;
            Ok(RecordItem {
                id: row.get(0)?,
                ext_key: row.get(1)?,
                data: serde_json::from_str(&data).unwrap_or_default(),
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn update_data(conn: &Connection, id: i64, data: &Map<String, Value>) -> AppResult<()> {
    let affected = conn.execute(
        "UPDATE records SET data = ?1, updated_at = ?2 WHERE id = ?3",
        params![serde_json::to_string(data).unwrap_or_else(|_| "{}".into()), now_str(), id],
    )?;

    if affected == 0 {
        return Err(AppError::not_found("记录不存在"));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    let affected = conn.execute("DELETE FROM records WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(AppError::not_found("记录不存在"));
    }
    Ok(())
}

pub fn count(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?)
}

pub fn list_batches(conn: &Connection, limit: i64) -> AppResult<Vec<ImportBatch>> {
    let mut stmt = conn.prepare(
        "SELECT b.id, b.filename, b.total_rows, b.inserted, b.updated, b.skipped, b.failed,
                b.operator_id, COALESCE(u.display_name, ''), b.created_at
         FROM import_batches b
         LEFT JOIN users u ON u.id = b.operator_id
         ORDER BY b.id DESC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map(params![limit.clamp(1, 200)], |row| {
        Ok(ImportBatch {
            id: row.get(0)?,
            filename: row.get(1)?,
            total_rows: row.get(2)?,
            inserted: row.get(3)?,
            updated: row.get(4)?,
            skipped: row.get(5)?,
            failed: row.get(6)?,
            operator_id: row.get(7)?,
            operator_name: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
