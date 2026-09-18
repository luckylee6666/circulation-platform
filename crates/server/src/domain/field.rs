use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::util::now_str;

pub const TYPE_TEXT: &str = "text";
pub const TYPE_NUMBER: &str = "number";
pub const TYPE_DATE: &str = "date";
pub const TYPE_SELECT: &str = "select";
pub const TYPE_BOOL: &str = "bool";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub id: i64,
    pub code: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub is_unique_key: bool,
    pub options: Vec<String>,
    pub sort: i64,
    pub show_in_list: bool,
    pub searchable: bool,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInput {
    pub code: String,
    pub label: String,
    #[serde(default = "default_type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub is_unique_key: bool,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub sort: i64,
    #[serde(default = "default_true")]
    pub show_in_list: bool,
    #[serde(default = "default_true")]
    pub searchable: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_type() -> String {
    TYPE_TEXT.to_string()
}

fn default_true() -> bool {
    true
}

const ALL_TYPES: [&str; 5] = [TYPE_TEXT, TYPE_NUMBER, TYPE_DATE, TYPE_SELECT, TYPE_BOOL];

impl FieldInput {
    pub fn validate(&self) -> AppResult<()> {
        let code = self.code.trim();
        if code.is_empty() {
            return Err(AppError::bad_request("字段标识不能为空"));
        }
        if !code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(AppError::bad_request(
                "字段标识只能用英文字母、数字、下划线或短横线",
            ));
        }
        if self.label.trim().is_empty() {
            return Err(AppError::bad_request("字段名称不能为空"));
        }
        if !ALL_TYPES.contains(&self.field_type.as_str()) {
            return Err(AppError::bad_request(format!(
                "不支持的字段类型：{}",
                self.field_type
            )));
        }
        if self.field_type == TYPE_SELECT && self.options.is_empty() {
            return Err(AppError::bad_request("下拉选项至少要有一个选项"));
        }
        Ok(())
    }
}

pub fn list(conn: &Connection, only_enabled: bool) -> AppResult<Vec<FieldDef>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, label, field_type, required, is_unique_key, options, sort,
                show_in_list, searchable, enabled, created_at
         FROM field_defs
         WHERE (?1 = 0 OR enabled = 1)
         ORDER BY sort, id",
    )?;

    let rows = stmt.query_map(params![i64::from(only_enabled)], map_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn find(conn: &Connection, id: i64) -> AppResult<Option<FieldDef>> {
    conn.query_row(
        "SELECT id, code, label, field_type, required, is_unique_key, options, sort,
                show_in_list, searchable, enabled, created_at
         FROM field_defs WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// 标为唯一键的字段，导入时用它来判重。
pub fn unique_key_field(conn: &Connection) -> AppResult<Option<FieldDef>> {
    conn.query_row(
        "SELECT id, code, label, field_type, required, is_unique_key, options, sort,
                show_in_list, searchable, enabled, created_at
         FROM field_defs WHERE is_unique_key = 1 LIMIT 1",
        [],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn create(conn: &Connection, input: &FieldInput) -> AppResult<i64> {
    input.validate()?;

    // 唯一键只能有一个，否则导入判重没有依据
    if input.is_unique_key {
        conn.execute("UPDATE field_defs SET is_unique_key = 0", [])?;
    }

    conn.execute(
        "INSERT INTO field_defs (code, label, field_type, required, is_unique_key, options,
                                 sort, show_in_list, searchable, enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            input.code.trim(),
            input.label.trim(),
            input.field_type,
            i64::from(input.required),
            i64::from(input.is_unique_key),
            serde_json::to_string(&input.options).unwrap_or_else(|_| "[]".into()),
            input.sort,
            i64::from(input.show_in_list),
            i64::from(input.searchable),
            i64::from(input.enabled),
            now_str()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, input: &FieldInput) -> AppResult<()> {
    input.validate()?;

    let exists = find(conn, id)?.ok_or_else(|| AppError::not_found("字段不存在"))?;

    if input.is_unique_key {
        conn.execute("UPDATE field_defs SET is_unique_key = 0 WHERE id != ?1", params![id])?;
    }

    // 唯一键变更后，历史记录的 ext_key 语义就变了，因此不允许改动已经用过的标识
    let code = if exists.code == input.code.trim() {
        exists.code.clone()
    } else {
        let used: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE json_extract(data, '$.' || ?1) IS NOT NULL)",
            params![exists.code],
            |row| row.get(0),
        )?;
        if used > 0 {
            return Err(AppError::bad_request(
                "该字段已有数据，不能再修改标识；如需调整请新建字段",
            ));
        }
        input.code.trim().to_string()
    };

    conn.execute(
        "UPDATE field_defs SET code = ?1, label = ?2, field_type = ?3, required = ?4,
                               is_unique_key = ?5, options = ?6, sort = ?7,
                               show_in_list = ?8, searchable = ?9, enabled = ?10
         WHERE id = ?11",
        params![
            code,
            input.label.trim(),
            input.field_type,
            i64::from(input.required),
            i64::from(input.is_unique_key),
            serde_json::to_string(&input.options).unwrap_or_else(|_| "[]".into()),
            input.sort,
            i64::from(input.show_in_list),
            i64::from(input.searchable),
            i64::from(input.enabled),
            id
        ],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    let field = find(conn, id)?.ok_or_else(|| AppError::not_found("字段不存在"))?;

    let used: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM records WHERE json_extract(data, '$.' || ?1) IS NOT NULL)",
        params![field.code],
        |row| row.get(0),
    )?;
    if used > 0 {
        return Err(AppError::bad_request(
            "该字段下已经有数据，不能删除；可以改为「停用」让它不再出现在表单里",
        ));
    }

    conn.execute("DELETE FROM field_defs WHERE id = ?1", params![id])?;
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FieldDef> {
    let options_raw: String = row.get(6)?;

    Ok(FieldDef {
        id: row.get(0)?,
        code: row.get(1)?,
        label: row.get(2)?,
        field_type: row.get(3)?,
        required: row.get::<_, i64>(4)? != 0,
        is_unique_key: row.get::<_, i64>(5)? != 0,
        options: serde_json::from_str(&options_raw).unwrap_or_default(),
        sort: row.get(7)?,
        show_in_list: row.get::<_, i64>(8)? != 0,
        searchable: row.get::<_, i64>(9)? != 0,
        enabled: row.get::<_, i64>(10)? != 0,
        created_at: row.get(11)?,
    })
}
