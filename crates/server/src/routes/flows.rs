use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::auth::permission::AuditEntry;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::flow::engine::TaskAction;
use crate::domain::flow::{self, FlowConfig};
use crate::error::{AppError, AppResult};
use crate::extract::ClientIp;
use crate::state::AppState;
use crate::util::now_str;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/flows/defs", get(list_defs).post(create_def))
        .route(
            "/flows/defs/{id}",
            get(get_def).put(update_def).delete(delete_def),
        )
        .route("/flows", post(create_instance))
        .route("/flows/instances", get(list_instances))
        .route("/flows/instances/{id}", get(get_instance))
        .route("/flows/instances/{id}/terminate", post(terminate))
        .route("/flows/tasks", get(list_tasks))
        .route("/flows/tasks/{id}/complete", post(complete_task))
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDefView {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub enabled: bool,
    pub is_default: bool,
    pub config: FlowConfig,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefInput {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub config: FlowConfig,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceInput {
    pub def_id: i64,
    pub title: String,
    #[serde(default)]
    pub record_id: Option<i64>,
    #[serde(default)]
    pub form: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteInput {
    /// complete / pass / reject
    pub action: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub assigned_to: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeQuery {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Deserialize)]
pub struct TerminateInput {
    #[serde(default)]
    pub reason: String,
}

fn map_def(row: &rusqlite::Row<'_>) -> rusqlite::Result<FlowDefView> {
    let raw: String = row.get(7)?;
    Ok(FlowDefView {
        id: row.get(0)?,
        code: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        version: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        is_default: row.get::<_, i64>(6)? != 0,
        config: serde_json::from_str(&raw).unwrap_or(FlowConfig {
            steps: vec![],
            rules: vec![],
        }),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

const DEF_SELECT: &str = "SELECT id, code, name, description, version, enabled, is_default,
                                 config, created_at, updated_at FROM flow_defs";

// ---------------- 流程定义 ----------------

async fn list_defs(
    State(state): State<AppState>,
    _current: CurrentUser,
) -> AppResult<Json<Vec<FlowDefView>>> {
    let defs = db::run(state.pool.clone(), move |conn| {
        let mut stmt = conn.prepare(&format!("{DEF_SELECT} ORDER BY id"))?;
        let rows = stmt.query_map([], map_def)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .await?;

    Ok(Json(defs))
}

async fn get_def(
    State(state): State<AppState>,
    _current: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<FlowDefView>> {
    let def = db::run(state.pool.clone(), move |conn| {
        conn.query_row(&format!("{DEF_SELECT} WHERE id = ?1"), params![id], map_def)
            .optional()
            .map_err(Into::into)
    })
    .await?
    .ok_or_else(|| AppError::not_found("流程定义不存在"))?;

    Ok(Json(def))
}

async fn create_def(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Json(input): Json<DefInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("flow:def:manage")?;
    validate_def(&input)?;

    let name = input.name.clone();
    let id = db::run(state.pool.clone(), move |conn| {
        let id = write_def(conn, None, &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "新增流程",
            target_type: "flow_def",
            target_id: &id.to_string(),
            detail: &name,
            ip: &ip,
        }
        .record(conn);
        Ok(id)
    })
    .await?;

    Ok(Json(json!({ "id": id })))
}

async fn update_def(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(input): Json<DefInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("flow:def:manage")?;
    validate_def(&input)?;

    let name = input.name.clone();
    db::run(state.pool.clone(), move |conn| {
        write_def(conn, Some(id), &input)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改流程",
            target_type: "flow_def",
            target_id: &id.to_string(),
            detail: &name,
            ip: &ip,
        }
        .record(conn);
        Ok(())
    })
    .await?;

    Ok(Json(json!({ "ok": true })))
}

async fn delete_def(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("flow:def:manage")?;

    db::run(state.pool.clone(), move |conn| {
        let used: i64 = conn.query_row(
            "SELECT COUNT(*) FROM flow_instances WHERE def_id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        if used > 0 {
            return Err(AppError::bad_request(format!(
                "已经有 {used} 个流转在用这个流程，不能删除；可以改为「停用」"
            )));
        }

        conn.execute("DELETE FROM flow_defs WHERE id = ?1", params![id])?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "删除流程",
            target_type: "flow_def",
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

fn validate_def(input: &DefInput) -> AppResult<()> {
    if input.code.trim().is_empty() {
        return Err(AppError::bad_request("流程标识不能为空"));
    }
    if input.name.trim().is_empty() {
        return Err(AppError::bad_request("流程名称不能为空"));
    }
    input.config.validate()
}

fn write_def(conn: &Connection, id: Option<i64>, input: &DefInput) -> AppResult<i64> {
    let config = serde_json::to_string(&input.config)
        .map_err(|err| AppError::bad_request(format!("流程配置序列化失败：{err}")))?;
    let now = now_str();

    // 默认流程只有一个
    if input.is_default {
        conn.execute("UPDATE flow_defs SET is_default = 0", [])?;
    }

    let id = match id {
        Some(id) => {
            let affected = conn.execute(
                "UPDATE flow_defs SET code = ?1, name = ?2, description = ?3, config = ?4,
                                      enabled = ?5, is_default = ?6, version = version + 1,
                                      updated_at = ?7
                 WHERE id = ?8",
                params![
                    input.code.trim(),
                    input.name.trim(),
                    input.description.trim(),
                    config,
                    i64::from(input.enabled),
                    i64::from(input.is_default),
                    now,
                    id
                ],
            )?;
            if affected == 0 {
                return Err(AppError::not_found("流程定义不存在"));
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO flow_defs (code, name, description, version, enabled, is_default,
                                        config, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?7)",
                params![
                    input.code.trim(),
                    input.name.trim(),
                    input.description.trim(),
                    i64::from(input.enabled),
                    i64::from(input.is_default),
                    config,
                    now
                ],
            )?;
            conn.last_insert_rowid()
        }
    };

    Ok(id)
}

// ---------------- 流转实例 ----------------

async fn create_instance(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(input): Json<CreateInstanceInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("flow:create")?;

    if input.title.trim().is_empty() {
        return Err(AppError::bad_request("请填写事项标题"));
    }

    let id = db::run(state.pool.clone(), move |conn| {
        flow::service::create_instance(
            conn,
            input.def_id,
            &input.title,
            input.record_id,
            &input.form,
            current.id,
            &current.display_name,
        )
    })
    .await?;

    state.publish("flow");
    Ok(Json(json!({ "id": id })))
}

async fn list_instances(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<ScopeQuery>,
) -> AppResult<Json<Vec<flow::service::InstanceItem>>> {
    current.require("data:view")?;

    let scope = query.scope.clone();
    // 「全部」需要额外权限，普通用户只能看与自己相关的
    if scope == "all" && !current.has("flow:terminate") {
        return Err(AppError::forbidden("只有调度员以上角色可以查看全部流转"));
    }

    let user_id = current.id;
    let items = db::run(state.pool.clone(), move |conn| {
        flow::service::list_instances(conn, &scope, user_id, 200)
    })
    .await?;

    Ok(Json(items))
}

async fn get_instance(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<i64>,
) -> AppResult<Json<flow::service::InstanceDetail>> {
    current.require("data:view")?;

    let viewer = current.id;
    let detail = db::run(state.pool.clone(), move |conn| {
        flow::service::instance_detail(conn, id, viewer)
    })
    .await?;

    Ok(Json(detail))
}

async fn terminate(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<i64>,
    Json(input): Json<TerminateInput>,
) -> AppResult<Json<serde_json::Value>> {
    current.require("flow:terminate")?;

    db::run(state.pool.clone(), move |conn| {
        flow::service::terminate_instance(
            conn,
            id,
            current.id,
            &current.display_name,
            &input.reason,
        )
    })
    .await?;

    state.publish("flow");
    Ok(Json(json!({ "ok": true })))
}

// ---------------- 任务 ----------------

async fn list_tasks(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<ScopeQuery>,
) -> AppResult<Json<Vec<flow::service::TaskItem>>> {
    let user_id = current.id;
    let done = query.done;

    let tasks = db::run(state.pool.clone(), move |conn| {
        flow::service::my_tasks(conn, user_id, done, 200)
    })
    .await?;

    Ok(Json(tasks))
}

async fn complete_task(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<i64>,
    Json(input): Json<CompleteInput>,
) -> AppResult<Json<serde_json::Value>> {
    let action = match input.action.as_str() {
        "complete" => TaskAction::Complete,
        "pass" => TaskAction::Pass,
        "reject" => TaskAction::Reject,
        other => return Err(AppError::bad_request(format!("不支持的操作：{other}"))),
    };

    let comment = input.comment.clone();
    let assigned_to = input.assigned_to.clone();
    let actor_id = current.id;
    let actor_name = current.display_name.clone();

    let instance_id = db::run(state.pool.clone(), move |conn| {
        flow::service::complete_task(conn, id, actor_id, &actor_name, action, &comment, &assigned_to)
    })
    .await?;

    state.publish("flow");
    Ok(Json(json!({ "ok": true, "instanceId": instance_id })))
}
