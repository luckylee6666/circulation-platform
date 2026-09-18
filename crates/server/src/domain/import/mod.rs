pub mod parse;
pub mod validate;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::domain::field::{self, FieldDef};
use crate::error::{AppError, AppResult};
use crate::util::{headers_fingerprint, now_str, normalize_header};

const SESSION_TTL_MINUTES: i64 = 60;
const PREVIEW_LIMIT: usize = 50;
const COMMIT_ERROR_LIMIT: usize = 200;

// ---------------- 上传会话 ----------------

/// 解析好的表格暂存在内存里，避免前端反复回传整个文件。
pub struct ImportSession {
    pub id: String,
    pub user_id: i64,
    pub source_name: String,
    pub rows: Vec<Vec<String>>,
    pub created_at: DateTime<Local>,
}

#[derive(Default)]
pub struct ImportStore {
    sessions: Mutex<HashMap<String, Arc<ImportSession>>>,
}

impl ImportStore {
    pub fn insert(
        &self,
        user_id: i64,
        source_name: String,
        rows: Vec<Vec<String>>,
    ) -> Arc<ImportSession> {
        let session = Arc::new(ImportSession {
            id: Uuid::new_v4().to_string(),
            user_id,
            source_name,
            rows,
            created_at: Local::now(),
        });

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.retain(|_, item| {
                (Local::now() - item.created_at).num_minutes() < SESSION_TTL_MINUTES
            });
            sessions.insert(session.id.clone(), session.clone());
        }

        session
    }

    pub fn get(&self, id: &str, user_id: i64) -> AppResult<Arc<ImportSession>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| AppError::internal("导入状态异常，请重新上传文件"))?;

        let session = sessions
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::not_found("导入会话已过期，请重新上传文件"))?;

        if session.user_id != user_id {
            return Err(AppError::forbidden("不能操作他人发起的导入"));
        }
        Ok(session)
    }

    pub fn remove(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(id);
        }
    }
}

// ---------------- 请求 / 响应 ----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dedupe {
    /// 唯一键已存在则覆盖更新
    Upsert,
    /// 唯一键已存在则跳过
    Skip,
    /// 全部当新数据插入（唯一键会加后缀避免冲突）
    Insert,
}

impl Dedupe {
    pub fn as_str(self) -> &'static str {
        match self {
            Dedupe::Upsert => "upsert",
            Dedupe::Skip => "skip",
            Dedupe::Insert => "insert",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "skip" => Dedupe::Skip,
            "insert" => Dedupe::Insert,
            _ => Dedupe::Upsert,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingRequest {
    pub header_row: usize,
    pub mapping: HashMap<String, usize>,
    pub dedupe: Dedupe,
    /// 提交时同时把这次映射存成模板
    #[serde(default)]
    pub template_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowStatus {
    Insert,
    Update,
    Skip,
    Invalid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub row_number: usize,
    pub status: RowStatus,
    pub errors: Vec<String>,
    pub data: Map<String, Value>,
    pub ext_key: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSummary {
    pub total: usize,
    pub insert: usize,
    pub update: usize,
    pub skip: usize,
    pub invalid: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub summary: PreviewSummary,
    pub rows: Vec<PreviewRow>,
    pub mapping_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    pub batch_id: i64,
    pub total: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<PreviewRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTemplate {
    pub id: i64,
    pub name: String,
    pub header_fingerprint: String,
    pub mapping: HashMap<String, usize>,
    pub dedupe: String,
    pub updated_at: String,
}

// ---------------- 核心流程 ----------------

pub struct Prepared {
    pub headers: Vec<String>,
    pub rows: Vec<PreparedRow>,
    pub mapping_errors: Vec<String>,
}

/// 按映射把原始行转成待入库数据，并逐行判定状态。
pub fn prepare(
    conn: &Connection,
    session: &ImportSession,
    fields: &[FieldDef],
    request: &MappingRequest,
) -> AppResult<Prepared> {
    if request.header_row >= session.rows.len() {
        return Err(AppError::bad_request("表头所在行超出了数据范围"));
    }

    let headers = session.rows[request.header_row].clone();
    let unique_field = fields.iter().find(|item| item.is_unique_key);
    let mut mapping_errors = Vec::new();

    for field in fields {
        if field.required && !request.mapping.contains_key(&field.code) {
            mapping_errors.push(format!("必填字段「{}」还没有选择对应的列", field.label));
        }
    }

    if let Some(unique) = unique_field
        && !request.mapping.contains_key(&unique.code)
    {
        mapping_errors.push(format!(
            "唯一键字段「{}」还没有选择对应的列，无法判断数据是否重复",
            unique.label
        ));
    }

    for (code, index) in &request.mapping {
        if *index >= headers.len() {
            mapping_errors.push(format!("字段「{code}」映射到了不存在的列"));
        }
    }

    let mut rows: Vec<PreparedRow> = Vec::new();
    let mut first_seen: HashMap<String, usize> = HashMap::new();
    let mut ext_keys: Vec<String> = Vec::new();

    for (offset, raw_row) in session.rows.iter().enumerate().skip(request.header_row + 1) {
        let row_number = offset + 1; // 与 Excel 显示的行号一致

        if raw_row.iter().all(|cell| cell.trim().is_empty()) {
            continue;
        }

        let mut data = Map::new();
        let mut errors = Vec::new();

        for field in fields {
            let Some(index) = request.mapping.get(&field.code).copied() else {
                continue;
            };
            let raw = raw_row.get(index).map(String::as_str).unwrap_or("");

            match validate::coerce(field, raw) {
                Ok(value) => {
                    data.insert(field.code.clone(), value);
                }
                Err(message) => errors.push(format!("{}：{message}", field.label)),
            }
        }

        let ext_key = build_ext_key(unique_field, &data, &mut errors);

        // 同一个文件里出现重复唯一键时，只保留第一条，其余标错
        match first_seen.get(&ext_key) {
            Some(first) => errors.push(format!("唯一键与第 {first} 行重复")),
            None => {
                first_seen.insert(ext_key.clone(), row_number);
                ext_keys.push(ext_key.clone());
            }
        }

        rows.push(PreparedRow {
            row_number,
            ext_key,
            data,
            errors,
            // 逐行解析时还不知道库里有没有同键记录，统一在下面判定
            status: RowStatus::Insert,
        });
    }

    let existing = existing_keys(conn, &ext_keys)?;
    for row in &mut rows {
        row.status = if !row.errors.is_empty() {
            RowStatus::Invalid
        } else if existing.contains(&row.ext_key) {
            match request.dedupe {
                Dedupe::Skip => RowStatus::Skip,
                Dedupe::Insert => RowStatus::Insert,
                Dedupe::Upsert => RowStatus::Update,
            }
        } else {
            RowStatus::Insert
        };
    }

    Ok(Prepared {
        headers,
        rows,
        mapping_errors,
    })
}

pub fn preview(prepared: &Prepared) -> Preview {
    let mut summary = PreviewSummary {
        total: prepared.rows.len(),
        ..PreviewSummary::default()
    };

    for row in &prepared.rows {
        match row.status {
            RowStatus::Insert => summary.insert += 1,
            RowStatus::Update => summary.update += 1,
            RowStatus::Skip => summary.skip += 1,
            RowStatus::Invalid => summary.invalid += 1,
        }
    }

    // 有问题的行优先展示，让人一眼看到要修什么
    let mut ordered: Vec<&PreparedRow> = prepared.rows.iter().collect();
    ordered.sort_by_key(|row| match row.status {
        RowStatus::Invalid => 0,
        RowStatus::Update => 1,
        RowStatus::Insert => 2,
        RowStatus::Skip => 3,
    });

    let rows = ordered
        .into_iter()
        .take(PREVIEW_LIMIT)
        .map(|row| PreviewRow {
            row_number: row.row_number,
            status: row.status,
            errors: row.errors.clone(),
            data: row.data.clone(),
            ext_key: row.ext_key.clone(),
        })
        .collect();

    Preview {
        summary,
        rows,
        mapping_errors: prepared.mapping_errors.clone(),
    }
}

pub fn commit(
    conn: &mut Connection,
    prepared: &Prepared,
    request: &MappingRequest,
    operator_id: i64,
    source_name: &str,
) -> AppResult<CommitOutcome> {
    let now = now_str();
    let batch_id;

    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut errors: Vec<PreviewRow> = Vec::new();

    {
        let tx = conn.transaction()?;

        // 「全部新增」时唯一键可能与已有数据撞车，这里统一加后缀
        let dedupe = request.dedupe;
        let needs_suffix = dedupe == Dedupe::Insert;

        for row in &prepared.rows {
            match row.status {
                RowStatus::Invalid => {
                    failed += 1;
                    if errors.len() < COMMIT_ERROR_LIMIT {
                        errors.push(to_preview_row(row));
                    }
                }
                RowStatus::Skip => skipped += 1,
                RowStatus::Insert => {
                    let ext_key = if needs_suffix {
                        unique_ext_key(&tx, &row.ext_key)?
                    } else {
                        row.ext_key.clone()
                    };

                    tx.execute(
                        "INSERT INTO records (ext_key, batch_id, data, created_by, created_at, updated_at)
                         VALUES (?1, NULL, ?2, ?3, ?4, ?4)
                         ON CONFLICT(ext_key) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
                        params![ext_key, serde_json::to_string(&row.data).unwrap(), operator_id, now],
                    )?;
                    inserted += 1;
                }
                RowStatus::Update => {
                    let changed = tx.execute(
                        "UPDATE records SET data = ?1, updated_at = ?2 WHERE ext_key = ?3",
                        params![serde_json::to_string(&row.data).unwrap(), now, row.ext_key],
                    )?;
                    if changed == 0 {
                        tx.execute(
                            "INSERT INTO records (ext_key, batch_id, data, created_by, created_at, updated_at)
                             VALUES (?1, NULL, ?2, ?3, ?4, ?4)",
                            params![row.ext_key, serde_json::to_string(&row.data).unwrap(), operator_id, now],
                        )?;
                        inserted += 1;
                    } else {
                        updated += 1;
                    }
                }
            }
        }

        let errors_json = serde_json::to_string(&errors).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO import_batches (filename, total_rows, inserted, updated, skipped, failed,
                                         errors, operator_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                source_name,
                prepared.rows.len() as i64,
                inserted as i64,
                updated as i64,
                skipped as i64,
                failed as i64,
                errors_json,
                operator_id,
                now
            ],
        )?;
        batch_id = tx.last_insert_rowid();

        // 把本批次写入的记录挂上批次号，便于回溯
        if inserted > 0 || updated > 0 {
            tx.execute(
                "UPDATE records SET batch_id = ?1
                 WHERE batch_id IS NULL
                   AND ext_key IN (SELECT value FROM json_each(?2))",
                params![batch_id, serde_json::to_string(&prepared.rows.iter().map(|r| r.ext_key.clone()).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into())],
            )?;
        }

        tx.commit()?;
    }

    Ok(CommitOutcome {
        batch_id,
        total: prepared.rows.len(),
        inserted,
        updated,
        skipped,
        failed,
        errors,
    })
}

// ---------------- 模板 ----------------

pub fn find_template(conn: &Connection, headers: &[String]) -> AppResult<Option<ImportTemplate>> {
    let fingerprint = headers_fingerprint(headers);

    conn.query_row(
        "SELECT id, name, header_fingerprint, mapping, dedupe_strategy, updated_at
         FROM import_templates WHERE header_fingerprint = ?1",
        params![fingerprint],
        |row| {
            let mapping: String = row.get(3)?;
            Ok(ImportTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                header_fingerprint: row.get(2)?,
                mapping: serde_json::from_str(&mapping).unwrap_or_default(),
                dedupe: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

pub fn save_template(
    conn: &Connection,
    name: &str,
    headers: &[String],
    mapping: &HashMap<String, usize>,
    dedupe: Dedupe,
) -> AppResult<i64> {
    let fingerprint = headers_fingerprint(headers);
    let mapping_json = serde_json::to_string(mapping).unwrap_or_else(|_| "{}".into());
    let now = now_str();

    conn.execute(
        "INSERT INTO import_templates (name, header_fingerprint, mapping, dedupe_strategy, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(header_fingerprint) DO UPDATE SET
             name = excluded.name, mapping = excluded.mapping,
             dedupe_strategy = excluded.dedupe_strategy, updated_at = excluded.updated_at",
        params![name.trim(), fingerprint, mapping_json, dedupe.as_str(), now],
    )?;

    let id: i64 = conn.query_row(
        "SELECT id FROM import_templates WHERE header_fingerprint = ?1",
        params![fingerprint],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn list_templates(conn: &Connection) -> AppResult<Vec<ImportTemplate>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, header_fingerprint, mapping, dedupe_strategy, updated_at
         FROM import_templates ORDER BY updated_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let mapping: String = row.get(3)?;
        Ok(ImportTemplate {
            id: row.get(0)?,
            name: row.get(1)?,
            header_fingerprint: row.get(2)?,
            mapping: serde_json::from_str(&mapping).unwrap_or_default(),
            dedupe: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 按字段名称/标识自动配对列，让用户少点几下。
pub fn suggest_mapping(headers: &[String], fields: &[FieldDef]) -> HashMap<String, usize> {
    let normalized: Vec<String> = headers.iter().map(|header| normalize_header(header)).collect();
    let mut mapping = HashMap::new();

    for field in fields {
        let target = normalize_header(&field.label);
        if let Some(index) = normalized.iter().position(|header| *header == target) {
            mapping.insert(field.code.clone(), index);
        }
    }

    for field in fields {
        if mapping.contains_key(&field.code) {
            continue;
        }
        let target = normalize_header(&field.code);
        if let Some(index) = normalized.iter().position(|header| *header == target) {
            mapping.insert(field.code.clone(), index);
        }
    }

    mapping
}

// ---------------- 内部辅助 ----------------

pub struct PreparedRow {
    pub row_number: usize,
    pub ext_key: String,
    pub data: Map<String, Value>,
    pub errors: Vec<String>,
    pub status: RowStatus,
}

fn to_preview_row(row: &PreparedRow) -> PreviewRow {
    PreviewRow {
        row_number: row.row_number,
        status: row.status,
        errors: row.errors.clone(),
        data: row.data.clone(),
        ext_key: row.ext_key.clone(),
    }
}

fn build_ext_key(
    unique_field: Option<&FieldDef>,
    data: &Map<String, Value>,
    errors: &mut Vec<String>,
) -> String {
    let Some(field) = unique_field else {
        return format!("auto-{}", Uuid::new_v4());
    };

    match data.get(&field.code) {
        Some(Value::Null) | None => {
            errors.push(format!("唯一键「{}」为空", field.label));
            format!("auto-{}", Uuid::new_v4())
        }
        Some(value) => {
            let key = value_to_key(value);
            if key.is_empty() {
                errors.push(format!("唯一键「{}」为空", field.label));
                format!("auto-{}", Uuid::new_v4())
            } else {
                key
            }
        }
    }
}

fn value_to_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => if *flag { "是" } else { "否" }.to_string(),
        _ => String::new(),
    }
}

/// 分批查已存在的唯一键，避免 IN 里塞进上万个数。
fn existing_keys(conn: &Connection, keys: &[String]) -> AppResult<HashSet<String>> {
    let mut found = HashSet::new();

    for chunk in keys.chunks(500) {
        if chunk.is_empty() {
            continue;
        }

        let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT ext_key FROM records WHERE ext_key IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(chunk.iter()), |row| row.get::<_, String>(0))?;

        for key in rows {
            found.insert(key?);
        }
    }

    Ok(found)
}

/// 「全部新增」模式下如果唯一键已存在，追加序号让它能存进去。
fn unique_ext_key(conn: &Connection, base: &str) -> AppResult<String> {
    let exists = |key: &str| -> AppResult<bool> {
        let found: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE ext_key = ?1)",
            params![key],
            |row| row.get(0),
        )?;
        Ok(found > 0)
    };

    if !exists(base)? {
        return Ok(base.to_string());
    }

    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        if !exists(&candidate)? {
            return Ok(candidate);
        }
    }

    Ok(format!("{base}-{}", Uuid::new_v4()))
}

/// 供页面展示：当前唯一键字段，没有的话导入无法判重。
pub fn unique_key(conn: &Connection) -> AppResult<Option<FieldDef>> {
    field::unique_key_field(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::field::{FieldInput, TYPE_TEXT};

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::migrate::run(&mut conn).unwrap();
        conn
    }

    fn add_field(conn: &Connection, code: &str, label: &str, unique: bool) -> FieldDef {
        let input = FieldInput {
            code: code.into(),
            label: label.into(),
            field_type: TYPE_TEXT.into(),
            required: false,
            is_unique_key: unique,
            options: vec![],
            sort: 0,
            show_in_list: true,
            searchable: true,
            enabled: true,
        };
        let id = field::create(conn, &input).unwrap();
        field::find(conn, id).unwrap().unwrap()
    }

    fn session(rows: Vec<Vec<&str>>) -> ImportSession {
        ImportSession {
            id: "s".into(),
            user_id: 1,
            source_name: "t.csv".into(),
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(String::from).collect())
                .collect(),
            created_at: Local::now(),
        }
    }

    fn request(mapping: &[(&str, usize)], dedupe: Dedupe) -> MappingRequest {
        MappingRequest {
            header_row: 0,
            mapping: mapping
                .iter()
                .map(|(code, index)| ((*code).to_string(), *index))
                .collect(),
            dedupe,
            template_name: None,
        }
    }

    #[test]
    fn prepare_maps_and_validates_rows() {
        let conn = setup();
        let code = add_field(&conn, "code", "编号", true);
        let name = add_field(&conn, "name", "名称", false);
        let fields = vec![code, name];

        let sheet = session(vec![
            vec!["编号", "名称"],
            vec!["A-1", "螺丝"],
            vec!["A-2", ""],
        ]);

        let prepared = prepare(&conn, &sheet, &fields, &request(&[("code", 0), ("name", 1)], Dedupe::Upsert)).unwrap();

        assert!(prepared.mapping_errors.is_empty());
        assert_eq!(prepared.rows.len(), 2);
        assert_eq!(prepared.rows[0].ext_key, "A-1");
        assert_eq!(prepared.rows[0].status, RowStatus::Insert);
        assert_eq!(prepared.rows[1].data["name"], Value::Null);
    }

    #[test]
    fn missing_required_field_is_reported() {
        let conn = setup();
        let mut required = add_field(&conn, "code", "编号", true);
        required.required = true;

        let sheet = session(vec![vec!["编号", "名称"], vec!["A-1", "螺丝"]]);
        // 只把「名称」映射上，必填的「编号」没映射
        let name = add_field(&conn, "name", "名称", false);
        let prepared = prepare(
            &conn,
            &sheet,
            &[required, name],
            &request(&[("name", 1)], Dedupe::Upsert),
        )
        .unwrap();

        assert_eq!(prepared.mapping_errors.len(), 2, "必填和唯一键都应提示");
    }

    #[test]
    fn duplicate_keys_inside_one_file_are_flagged() {
        let conn = setup();
        let code = add_field(&conn, "code", "编号", true);

        let sheet = session(vec![
            vec!["编号"],
            vec!["A-1"],
            vec!["A-1"],
        ]);

        let prepared = prepare(&conn, &sheet, &[code], &request(&[("code", 0)], Dedupe::Upsert)).unwrap();
        assert_eq!(prepared.rows[0].status, RowStatus::Insert);
        assert_eq!(prepared.rows[1].status, RowStatus::Invalid);
        assert!(prepared.rows[1].errors[0].contains("第 2 行重复"));
    }

    #[test]
    fn existing_key_drives_update_and_skip() {
        let conn = setup();
        let code = add_field(&conn, "code", "编号", true);
        let name = add_field(&conn, "name", "名称", false);
        let fields = vec![code, name];

        conn.execute(
            "INSERT INTO records (ext_key, data, created_at, updated_at) VALUES ('A-1', '{}', '', '')",
            [],
        )
        .unwrap();

        let sheet = session(vec![vec!["编号", "名称"], vec!["A-1", "螺丝"]]);

        let upsert = prepare(&conn, &sheet, &fields, &request(&[("code", 0), ("name", 1)], Dedupe::Upsert)).unwrap();
        assert_eq!(upsert.rows[0].status, RowStatus::Update);

        let skip = prepare(&conn, &sheet, &fields, &request(&[("code", 0), ("name", 1)], Dedupe::Skip)).unwrap();
        assert_eq!(skip.rows[0].status, RowStatus::Skip);
    }

    #[test]
    fn commit_inserts_and_updates_records() {
        let mut conn = setup();
        let code = add_field(&conn, "code", "编号", true);
        let name = add_field(&conn, "name", "名称", false);
        let fields = vec![code, name];

        let sheet = session(vec![
            vec!["编号", "名称"],
            vec!["A-1", "螺丝"],
            vec!["A-2", "螺母"],
        ]);
        let req = request(&[("code", 0), ("name", 1)], Dedupe::Upsert);

        let prepared = prepare(&conn, &sheet, &fields, &req).unwrap();
        let outcome = commit(&mut conn, &prepared, &req, 1, "t.csv").unwrap();
        assert_eq!(outcome.inserted, 2);
        assert_eq!(outcome.updated, 0);

        // 再导一次同样的文件，应该变成更新而不是重复插入
        let again = prepare(&conn, &sheet, &fields, &req).unwrap();
        assert!(again.rows.iter().all(|row| row.status == RowStatus::Update));

        let outcome = commit(&mut conn, &again, &req, 1, "t.csv").unwrap();
        assert_eq!(outcome.updated, 2);
        assert_eq!(outcome.inserted, 0);

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0)).unwrap();
        assert_eq!(total, 2);
    }

    #[test]
    fn commit_skips_invalid_rows_and_records_errors() {
        let mut conn = setup();
        let code = add_field(&conn, "code", "编号", true);
        let name = add_field(&conn, "name", "名称", false);
        let fields = vec![code, name];

        let sheet = session(vec![
            vec!["编号", "名称"],
            vec!["A-1", "螺丝"],
            vec!["", "螺母"], // 唯一键为空，无法判重
            vec!["A-2", "垫片"],
        ]);
        let req = request(&[("code", 0), ("name", 1)], Dedupe::Upsert);

        let prepared = prepare(&conn, &sheet, &fields, &req).unwrap();
        assert_eq!(prepared.rows[1].status, RowStatus::Invalid);
        assert!(prepared.rows[1].errors[0].contains("唯一键"));

        let outcome = commit(&mut conn, &prepared, &req, 1, "t.csv").unwrap();
        assert_eq!(outcome.inserted, 2);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.errors.len(), 1);

        let batch_failed: i64 = conn
            .query_row(
                "SELECT failed FROM import_batches WHERE id = ?1",
                [outcome.batch_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(batch_failed, 1);
    }

    #[test]
    fn fully_blank_rows_are_ignored_instead_of_reported_as_errors() {
        let conn = setup();
        let code = add_field(&conn, "code", "编号", true);

        // 表格末尾经常拖一堆空行，不该因此报一堆错
        let sheet = session(vec![vec!["编号"], vec!["A-1"], vec![""], vec!["   "]]);
        let prepared = prepare(&conn, &sheet, &[code], &request(&[("code", 0)], Dedupe::Upsert)).unwrap();

        assert_eq!(prepared.rows.len(), 1);
        assert_eq!(prepared.rows[0].status, RowStatus::Insert);
    }

    #[test]
    fn suggest_mapping_matches_labels_and_codes() {
        let conn = setup();
        let code = add_field(&conn, "code", "编号", true);
        let name = add_field(&conn, "name", "物料名称", false);

        let headers = vec!["物料名称".to_string(), "编 号".to_string()];
        let mapping = suggest_mapping(&headers, &[code, name]);

        assert_eq!(mapping.get("name"), Some(&0));
        assert_eq!(mapping.get("code"), Some(&1), "带空格的中文表头也应匹配上");
    }

    #[test]
    fn template_is_reused_by_header_fingerprint() {
        let conn = setup();
        let headers = vec!["编号".to_string(), "名称".to_string()];
        let mut mapping = HashMap::new();
        mapping.insert("code".to_string(), 0usize);

        assert!(find_template(&conn, &headers).unwrap().is_none());

        save_template(&conn, "物料导入", &headers, &mapping, Dedupe::Skip).unwrap();
        let found = find_template(&conn, &headers).unwrap().expect("应能找到模板");
        assert_eq!(found.name, "物料导入");
        assert_eq!(found.dedupe, "skip");
        assert_eq!(found.mapping.get("code"), Some(&0));

        // 不同表头不应命中
        assert!(find_template(&conn, &["别的".to_string()]).unwrap().is_none());
    }
}
