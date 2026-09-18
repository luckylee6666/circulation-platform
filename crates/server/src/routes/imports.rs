use std::collections::HashMap;

use axum::extract::{Multipart, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::db;
use crate::domain::field;
use crate::domain::import::{self, Dedupe, ImportTemplate, MappingRequest};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// 上传后回给前端的预览行数：够看清表头在第几行即可。
const UPLOAD_PREVIEW_ROWS: usize = 15;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/imports/upload", post(upload))
        .route("/imports/paste", post(paste))
        .route("/imports/{id}/preview", post(preview))
        .route("/imports/{id}/commit", post(commit))
        .route("/imports/templates", get(list_templates))
        .route("/imports/batches", get(list_batches))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub session_id: String,
    pub source_name: String,
    /// 含表头在内的总行数
    pub total_rows: usize,
    /// 建议的表头所在行（0 起）
    pub header_row: usize,
    /// 前若干行原始数据，供用户确认表头位置
    pub preview_rows: Vec<Vec<String>>,
    pub suggested_mapping: HashMap<String, usize>,
    pub matched_template: Option<ImportTemplate>,
    /// 没有配置唯一键字段时，只能「全部新增」
    pub unique_key_label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteInput {
    pub text: String,
}

async fn upload(
    State(state): State<AppState>,
    current: CurrentUser,
    mut multipart: Multipart,
) -> AppResult<Json<UploadResult>> {
    current.require("data:import")?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(format!("读取上传内容失败：{err}")))?
    {
        let Some(file_name) = field.file_name().map(str::to_string) else {
            continue;
        };

        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::bad_request(format!("读取文件内容失败：{err}")))?;

        let sheet = import::parse::from_upload(&file_name, &bytes)?;
        return build_result(&state, current.id, sheet).await;
    }

    Err(AppError::bad_request("没有收到文件"))
}

async fn paste(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<PasteInput>,
) -> AppResult<Json<UploadResult>> {
    current.require("data:import")?;

    let sheet = import::parse::from_paste(&input.text)?;
    build_result(&state, current.id, sheet).await
}

async fn build_result(
    state: &AppState,
    user_id: i64,
    sheet: import::parse::ParsedSheet,
) -> AppResult<Json<UploadResult>> {
    let header_row = import::parse::suggest_header_row(&sheet.rows);
    let headers = sheet.rows[header_row].clone();
    let total_rows = sheet.rows.len();
    let preview_rows = sheet.rows.iter().take(UPLOAD_PREVIEW_ROWS).cloned().collect();

    let session = state
        .imports
        .insert(user_id, sheet.source_name.clone(), sheet.rows);

    let fingerprint_headers = headers.clone();
    let (fields, matched_template) = db::run(state.pool.clone(), move |conn| {
        let fields = field::list(conn, true)?;
        let template = import::find_template(conn, &fingerprint_headers)?;
        Ok((fields, template))
    })
    .await?;

    // 命中模板就直接用上次的映射，这是「第二次一键导入」的关键
    let suggested_mapping = match &matched_template {
        Some(template) => template.mapping.clone(),
        None => import::suggest_mapping(&headers, &fields),
    };

    let unique_key_label = fields
        .iter()
        .find(|item| item.is_unique_key)
        .map(|item| item.label.clone());

    Ok(Json(UploadResult {
        session_id: session.id.clone(),
        source_name: session.source_name.clone(),
        total_rows,
        header_row,
        preview_rows,
        suggested_mapping,
        matched_template,
        unique_key_label,
    }))
}

async fn preview(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(request): Json<MappingRequest>,
) -> AppResult<Json<import::Preview>> {
    current.require("data:import")?;

    let session = state.imports.get(&id, current.id)?;
    let result = db::run(state.pool.clone(), move |conn| {
        let fields = field::list(conn, true)?;
        let prepared = import::prepare(conn, &session, &fields, &request)?;
        Ok(import::preview(&prepared))
    })
    .await?;

    Ok(Json(result))
}

async fn commit(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<String>,
    Json(request): Json<MappingRequest>,
) -> AppResult<Json<import::CommitOutcome>> {
    current.require("data:import")?;

    let session = state.imports.get(&id, current.id)?;

    if request.dedupe == Dedupe::Upsert {
        // 覆盖更新必须能唯一确定一行，否则会把不相关的记录冲掉
        let has_unique = db::run(state.pool.clone(), move |conn| {
            Ok(field::unique_key_field(conn)?.is_some())
        })
        .await?;
        if !has_unique {
            return Err(AppError::bad_request(
                "还没有设置唯一键字段，无法按编号覆盖更新；请在「字段定义」里把编号列标记为唯一键",
            ));
        }
    }

    let source_name = session.source_name.clone();
    let operator = current.id;
    let template_name = request.template_name.clone();
    let headers = session.rows[request.header_row].clone();
    let mapping = request.mapping.clone();
    let dedupe = request.dedupe;

    let outcome = db::run(state.pool.clone(), move |conn| {
        let fields = field::list(conn, true)?;
        let prepared = import::prepare(conn, &session, &fields, &request)?;

        if !prepared.mapping_errors.is_empty() {
            return Err(AppError::bad_request(
                prepared.mapping_errors.join("；"),
            ));
        }

        let outcome = import::commit(conn, &prepared, &request, operator, &source_name)?;

        // 顺手把这次映射存成模板，下次同格式文件就能一键导入
        let name = template_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("{source_name} 的映射"));
        import::save_template(conn, &name, &headers, &mapping, dedupe)?;

        Ok(outcome)
    })
    .await?;

    state.imports.remove(&id);
    Ok(Json(outcome))
}

async fn list_templates(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<Vec<ImportTemplate>>> {
    current.require("data:import")?;
    let templates = db::run(state.pool.clone(), move |conn| {
        import::list_templates(conn)
    })
    .await?;
    Ok(Json(templates))
}

async fn list_batches(
    State(state): State<AppState>,
    current: CurrentUser,
) -> AppResult<Json<Vec<crate::domain::record::ImportBatch>>> {
    current.require("data:view")?;
    let batches = db::run(state.pool.clone(), move |conn| {
        crate::domain::record::list_batches(conn, 50)
    })
    .await?;
    Ok(Json(batches))
}
