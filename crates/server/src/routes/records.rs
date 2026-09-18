use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::auth::permission::AuditEntry;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::field::{self, FieldDef};
use crate::domain::record::{self, RecordItem, RecordPage};
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::state::AppState;

const EXPORT_LIMIT: i64 = 100_000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/records", get(list_records))
        .route("/records/export", get(export_records))
        .route(
            "/records/{id}",
            get(get_record).put(update_record).delete(delete_record),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    #[serde(default)]
    keyword: String,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateInput {
    pub data: Map<String, Value>,
}

async fn list_records(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<RecordPage>> {
    current.require("data:view")?;

    let keyword = query.keyword.trim().to_string();
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let result = db::run(state.pool.clone(), move |conn| {
        record::list(conn, &keyword, page, page_size)
    })
    .await?;

    Ok(Json(result))
}

async fn get_record(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<RecordItem>> {
    current.require("data:view")?;

    let item = db::run(state.pool.clone(), move |conn| record::find(conn, id))
        .await?
        .ok_or_else(|| AppError::not_found("记录不存在"))?;

    Ok(Json(item))
}

async fn update_record(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(input): Json<UpdateInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("data:edit")?;

    db::run(state.pool.clone(), move |conn| {
        // 界面上能填就能清空，必填校验必须在服务端兜住
        for field in field::list(conn, true)?.iter().filter(|item| item.required) {
            let empty = match input.data.get(&field.code) {
                None | Some(Value::Null) => true,
                Some(Value::String(text)) => text.trim().is_empty(),
                _ => false,
            };
            if empty {
                return Err(AppError::bad_request(format!("「{}」是必填项", field.label)));
            }
        }

        record::update_data(conn, id, &input.data)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改记录",
            target_type: "record",
            target_id: &id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_record(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("data:delete")?;

    db::run(state.pool.clone(), move |conn| {
        record::delete(conn, id)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "删除记录",
            target_type: "record",
            target_id: &id.to_string(),
            detail: "",
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn export_records(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Response> {
    current.require("data:export")?;

    let keyword = query.keyword.trim().to_string();
    let (fields, records) = db::run(state.pool.clone(), move |conn| {
        let fields = field::list(conn, false)?;
        let records = record::list_all(conn, &keyword, EXPORT_LIMIT)?;
        Ok((fields, records))
    })
    .await?;

    let csv = build_csv(&fields, &records);
    let filename = format!(
        "records-{}.csv",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );

    // Excel 打开 UTF-8 的 CSV 必须有 BOM，否则中文会变乱码
    let body = format!("\u{feff}{csv}");

    Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(body))
        .map_err(|err| AppError::internal(format!("生成导出文件失败：{err}")))
}

fn build_csv(fields: &[FieldDef], records: &[RecordItem]) -> String {
    let mut csv = String::new();

    write_row(
        &mut csv,
        &fields.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(),
    );

    for item in records {
        let row: Vec<String> = fields
            .iter()
            .map(|field| value_to_text(item.data.get(&field.code)))
            .collect();
        write_row(&mut csv, &row.iter().map(String::as_str).collect::<Vec<_>>());
    }

    csv
}

fn write_row(csv: &mut String, cells: &[&str]) {
    let escaped: Vec<String> = cells.iter().map(|cell| escape_csv(cell)).collect();
    csv.push_str(&escaped.join(","));
    csv.push_str("\r\n");
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn value_to_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => if *flag { "是" } else { "否" }.to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escapes_special_characters() {
        assert_eq!(escape_csv("普通"), "普通");
        assert_eq!(escape_csv("含,逗号"), "\"含,逗号\"");
        assert_eq!(escape_csv("含\"引号"), "\"含\"\"引号\"");
        assert_eq!(escape_csv("含\n换行"), "\"含\n换行\"");
    }

    #[test]
    fn values_are_rendered_for_excel() {
        assert_eq!(value_to_text(None), "");
        assert_eq!(value_to_text(Some(&Value::Null)), "");
        assert_eq!(value_to_text(Some(&json!(12))), "12");
        assert_eq!(value_to_text(Some(&json!("文本"))), "文本");
        assert_eq!(value_to_text(Some(&json!(true))), "是");
        assert_eq!(value_to_text(Some(&json!(false))), "否");
    }
}
