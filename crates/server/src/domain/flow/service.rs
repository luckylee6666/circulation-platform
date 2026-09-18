use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::{Map, Value};

use super::engine::{self, Plan, Roster, TaskAction, TaskRow, TaskStatus};
use super::{FlowConfig, StepKind};
use crate::domain::notify;
use crate::error::{AppError, AppResult};
use crate::util::now_str;

pub const STATUS_RUNNING: i64 = 1;
pub const STATUS_FINISHED: i64 = 2;
pub const STATUS_TERMINATED: i64 = 3;

const TASK_PENDING: i64 = 0;
const TASK_DONE: i64 = 1;
const TASK_CANCELLED: i64 = 2;

// ---------------- 对外结构 ----------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: i64,
    pub instance_id: i64,
    pub instance_code: String,
    pub title: String,
    pub step_key: String,
    pub step_name: String,
    pub step_type: String,
    pub status: i64,
    pub assignee_id: i64,
    pub assignee_name: String,
    pub action: String,
    pub comment: String,
    pub created_at: String,
    pub done_at: Option<String>,
    pub initiator_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceItem {
    pub id: i64,
    pub code: String,
    pub title: String,
    pub def_name: String,
    pub status: i64,
    pub current_step_key: String,
    pub current_step_name: String,
    pub initiator_id: i64,
    pub initiator_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub pending_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogItem {
    pub id: i64,
    pub actor_name: String,
    pub action: String,
    pub from_step: String,
    pub to_step: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDetail {
    pub id: i64,
    pub code: String,
    pub def_id: i64,
    pub def_name: String,
    pub title: String,
    pub status: i64,
    pub current_step_key: String,
    pub current_step_name: String,
    pub current_step_type: String,
    pub initiator_id: i64,
    pub initiator_name: String,
    pub record_id: Option<i64>,
    pub form: Map<String, Value>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub tasks: Vec<TaskItem>,
    pub logs: Vec<LogItem>,
    /// 当前登录用户在这一步待办的任务号；为 None 表示他此刻不能操作
    pub my_pending_task_id: Option<i64>,
}

// ---------------- 装配 ----------------

/// 角色 -> 成员，供引擎解析「按角色指派」。
pub fn load_roster(conn: &Connection) -> AppResult<Roster> {
    let mut stmt = conn.prepare(
        "SELECT r.code, ur.user_id
         FROM roles r
         JOIN user_roles ur ON ur.role_id = r.id
         JOIN users u ON u.id = ur.user_id AND u.status = 1
         ORDER BY r.code, ur.user_id",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut roles: HashMap<String, Vec<i64>> = HashMap::new();
    for row in rows {
        let (code, user_id) = row?;
        roles.entry(code).or_default().push(user_id);
    }

    Ok(Roster { roles })
}

struct InstanceRow {
    id: i64,
    def_id: i64,
    initiator_id: i64,
    status: i64,
    current_step_key: String,
    form: Map<String, Value>,
    record_id: Option<i64>,
    title: String,
}

fn load_instance(conn: &Connection, instance_id: i64) -> AppResult<InstanceRow> {
    conn.query_row(
        "SELECT id, def_id, initiator_id, status, current_step_key, form, record_id, title
         FROM flow_instances WHERE id = ?1",
        params![instance_id],
        |row| {
            let form: String = row.get(5)?;
            Ok(InstanceRow {
                id: row.get(0)?,
                def_id: row.get(1)?,
                initiator_id: row.get(2)?,
                status: row.get(3)?,
                current_step_key: row.get(4)?,
                form: serde_json::from_str(&form).unwrap_or_default(),
                record_id: row.get(6)?,
                title: row.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("流程不存在"))
}

fn load_def(conn: &Connection, def_id: i64) -> AppResult<(String, FlowConfig)> {
    let (name, config): (String, String) = conn
        .query_row(
            "SELECT name, config FROM flow_defs WHERE id = ?1",
            params![def_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::bad_request("流程定义已被删除"))?;

    Ok((name, FlowConfig::parse(&config)?))
}

fn load_step_tasks(conn: &Connection, instance_id: i64, step_key: &str) -> AppResult<Vec<TaskRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, assignee_id, status FROM flow_tasks
         WHERE instance_id = ?1 AND step_key = ?2 AND status != ?3",
    )?;

    let rows = stmt.query_map(params![instance_id, step_key, TASK_CANCELLED], |row| {
        let status: i64 = row.get(2)?;
        Ok(TaskRow {
            id: row.get(0)?,
            assignee_id: row.get(1)?,
            status: if status == TASK_DONE {
                TaskStatus::Done
            } else {
                TaskStatus::Pending
            },
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 把关联记录的数据合并进来，判断条件才能用到外部导入的字段。
fn flow_data(conn: &Connection, form: &Map<String, Value>, record_id: Option<i64>) -> AppResult<Map<String, Value>> {
    let mut data = form.clone();

    if let Some(record_id) = record_id {
        let raw: Option<String> = conn
            .query_row("SELECT data FROM records WHERE id = ?1", params![record_id], |row| {
                row.get(0)
            })
            .optional()?;

        if let Some(raw) = raw {
            let record: Map<String, Value> = serde_json::from_str(&raw).unwrap_or_default();
            for (key, value) in record {
                data.entry(key).or_insert(value);
            }
        }
    }

    Ok(data)
}

// ---------------- 发起 ----------------

pub fn create_instance(
    conn: &mut Connection,
    def_id: i64,
    title: &str,
    record_id: Option<i64>,
    form: &Map<String, Value>,
    initiator_id: i64,
    initiator_name: &str,
) -> AppResult<i64> {
    let (_, config) = load_def(conn, def_id)?;
    if config.steps.is_empty() {
        return Err(AppError::bad_request("流程定义没有配置步骤"));
    }

    let roster = load_roster(conn)?;
    let data = flow_data(conn, form, record_id)?;
    let plan = engine::start(&config, &roster, initiator_id, &data)
        .map_err(|err| AppError::bad_request(err.to_string()))?;

    let code = next_code(conn)?;
    let now = now_str();

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO flow_instances (code, def_id, record_id, title, initiator_id, status,
                                     current_step_key, form, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, ?8, ?8)",
        params![
            code,
            def_id,
            record_id,
            title.trim(),
            initiator_id,
            STATUS_RUNNING,
            serde_json::to_string(form).unwrap_or_else(|_| "{}".into()),
            now
        ],
    )?;
    let instance_id = tx.last_insert_rowid();

    apply_plan(&tx, instance_id, &config, &plan, initiator_id)?;

    let to_step = transition_target(&config, &plan);
    write_log(
        &tx,
        instance_id,
        Some(initiator_id),
        initiator_name,
        "发起",
        "",
        &to_step,
        "",
    )?;

    // 第一步的待办落到谁头上就提醒谁
    // 通知在事务里落库，前端靠轮询取回
    notify_new_tasks(
        &tx,
        &config,
        &plan.create_tasks,
        title,
        &format!("{initiator_name} 发起，等待你处理"),
        instance_id,
        initiator_id,
    )?;

    tx.commit()?;
    Ok(instance_id)
}

/// 给新产生的待办对应的处理人发提醒，跳过操作者本人。
fn notify_new_tasks(
    conn: &Connection,
    config: &FlowConfig,
    tasks: &[engine::NewTask],
    title: &str,
    content: &str,
    instance_id: i64,
    actor_id: i64,
) -> AppResult<Vec<i64>> {
    let targets: Vec<i64> = tasks
        .iter()
        .map(|task| task.assignee_id)
        .filter(|assignee| *assignee != actor_id)
        .collect();
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let step_name = tasks
        .first()
        .and_then(|task| config.step(&task.step_key))
        .map(|step| step.name.clone())
        .unwrap_or_default();

    notify::push_many(conn, &targets, |_| {
        notify::NewNotification::flow(
            notify::category::TASK_ASSIGNED,
            format!("新待办：{title}"),
            format!("{content}（当前环节：{step_name}）"),
            instance_id,
        )
    })
}

fn next_code(conn: &Connection) -> AppResult<String> {
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let prefix = format!("LL-{today}-");

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM flow_instances WHERE code LIKE ?1",
        params![format!("{prefix}%")],
        |row| row.get(0),
    )?;

    Ok(format!("{prefix}{:04}", count + 1))
}

fn step_display_name(config: &FlowConfig, key: &str) -> String {
    config
        .step(key)
        .map(|step| step.name.clone())
        .unwrap_or_else(|| key.to_string())
}

/// 时间线上「去往哪一步」的展示文案。
///
/// 注意区分三种情况：推进到某一步、流程结束、以及停在原地等其他人处理。
/// 最后一种必须留空——早先把它也写成「结束」，看起来像是流程已经办结了。
fn transition_target(config: &FlowConfig, plan: &Plan) -> String {
    if plan.finish {
        return "结束".to_string();
    }
    plan.advance_to
        .as_deref()
        .map(|key| step_display_name(config, key))
        .unwrap_or_default()
}

/// 把引擎给出的改动写进库：取消作废任务、创建新任务、推进或结束实例。
fn apply_plan(
    tx: &Transaction<'_>,
    instance_id: i64,
    config: &FlowConfig,
    plan: &Plan,
    actor_id: i64,
) -> AppResult<()> {
    let now = now_str();

    for task_id in &plan.cancel_tasks {
        tx.execute(
            "UPDATE flow_tasks SET status = ?1, done_at = ?2 WHERE id = ?3 AND status = ?4",
            params![TASK_CANCELLED, now, task_id, TASK_PENDING],
        )?;
    }

    for task in &plan.create_tasks {
        let Some(step) = config.step(&task.step_key) else {
            continue;
        };

        tx.execute(
            "INSERT INTO flow_tasks (instance_id, step_key, step_name, step_type, assignee_id,
                                     assigned_by, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                instance_id,
                step.key,
                step.name,
                kind_str(step.kind),
                task.assignee_id,
                actor_id,
                TASK_PENDING,
                now
            ],
        )?;
    }

    if plan.finish {
        tx.execute(
            "UPDATE flow_instances SET status = ?1, finished_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![STATUS_FINISHED, now, instance_id],
        )?;
    } else if let Some(step_key) = &plan.advance_to {
        tx.execute(
            "UPDATE flow_instances SET current_step_key = ?1, updated_at = ?2 WHERE id = ?3",
            params![step_key, now, instance_id],
        )?;
    }

    Ok(())
}

pub fn kind_str(kind: StepKind) -> &'static str {
    match kind {
        StepKind::Start => "start",
        StepKind::Dispatch => "dispatch",
        StepKind::Handle => "handle",
        StepKind::Confirm => "confirm",
    }
}

#[allow(clippy::too_many_arguments)]
fn write_log(
    tx: &Transaction<'_>,
    instance_id: i64,
    actor_id: Option<i64>,
    actor_name: &str,
    action: &str,
    from_step: &str,
    to_step: &str,
    detail: &str,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO flow_logs (instance_id, actor_id, actor_name, action, from_step, to_step, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![instance_id, actor_id, actor_name, action, from_step, to_step, detail, now_str()],
    )?;
    Ok(())
}

// ---------------- 办理 ----------------

pub fn complete_task(
    conn: &mut Connection,
    task_id: i64,
    actor_id: i64,
    actor_name: &str,
    action: TaskAction,
    comment: &str,
    assigned_to: &[i64],
) -> AppResult<i64> {
    let task = load_task(conn, task_id)?;

    if task.assignee_id != actor_id {
        return Err(AppError::forbidden("这个任务不是指派给你的"));
    }
    if task.status != TASK_PENDING {
        return Err(AppError::bad_request("该任务已经处理过了"));
    }

    let instance = load_instance(conn, task.instance_id)?;
    if instance.status != STATUS_RUNNING {
        return Err(AppError::bad_request("流程已经结束或已终止"));
    }

    let (_, config) = load_def(conn, instance.def_id)?;
    let roster = load_roster(conn)?;
    let data = flow_data(conn, &instance.form, instance.record_id)?;
    let tasks = load_step_tasks(conn, instance.id, &instance.current_step_key)?;

    let plan = engine::complete(
        &config,
        &roster,
        instance.initiator_id,
        &instance.current_step_key,
        &tasks,
        task_id,
        action,
        assigned_to,
        &data,
    )
    .map_err(|err| AppError::bad_request(err.to_string()))?;

    // 时间线上要能一眼看出这一步是分派还是承办，不能都叫「完成」
    let step_kind = config.step(&instance.current_step_key).map(|step| step.kind);
    let action_str = match action {
        TaskAction::Reject => "打回",
        TaskAction::Pass => "确认通过",
        TaskAction::Complete => match step_kind {
            Some(StepKind::Dispatch) => "分派",
            _ => "承办完成",
        },
    };

    let now = now_str();
    let result = serde_json::json!({ "assignedTo": assigned_to }).to_string();

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE flow_tasks SET status = ?1, action = ?2, comment = ?3, result = ?4, done_at = ?5
         WHERE id = ?6",
        params![TASK_DONE, action_str, comment.trim(), result, now, task_id],
    )?;

    apply_plan(&tx, instance.id, &config, &plan, actor_id)?;

    let to_step = transition_target(&config, &plan);
    write_log(
        &tx,
        instance.id,
        Some(actor_id),
        actor_name,
        action_str,
        &task.step_name,
        &to_step,
        comment.trim(),
    )?;

    notify_new_tasks(
        &tx,
        &config,
        &plan.create_tasks,
        &instance.title,
        &format!("{actor_name} 处理完转交给你"),
        instance.id,
        actor_id,
    )?;

    // 打回和办结只靠新待办是提醒不到的，得直接告诉发起人
    let initiator_notice = if action == TaskAction::Reject {
        Some((
            notify::category::TASK_REJECTED,
            format!("事项被打回：{}", instance.title),
            format!(
                "{actor_name} 打回：{}",
                if comment.trim().is_empty() { "未填写原因" } else { comment.trim() }
            ),
        ))
    } else if plan.finish {
        Some((
            notify::category::FLOW_FINISHED,
            format!("事项已办结：{}", instance.title),
            format!("{actor_name} 确认通过"),
        ))
    } else {
        None
    };

    if let Some((kind, title, content)) = initiator_notice {
        notify::push_many(&tx, &[instance.initiator_id], |_| {
            notify::NewNotification::flow(kind, title.clone(), content.clone(), instance.id)
        })?;
    }

    tx.commit()?;
    Ok(instance.id)
}

pub fn terminate_instance(
    conn: &mut Connection,
    instance_id: i64,
    actor_id: i64,
    actor_name: &str,
    reason: &str,
) -> AppResult<()> {
    let instance = load_instance(conn, instance_id)?;
    if instance.status != STATUS_RUNNING {
        return Err(AppError::bad_request("流程已经结束或已终止"));
    }

    // 待办被作废的人要知道这件事，否则会一直以为自己还有活没干
    let pending: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT assignee_id FROM flow_tasks
             WHERE instance_id = ?1 AND status = ?2 AND assignee_id <> ?3",
        )?;
        let rows = stmt.query_map(params![instance_id, TASK_PENDING, actor_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let now = now_str();
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE flow_tasks SET status = ?1, done_at = ?2 WHERE instance_id = ?3 AND status = ?4",
        params![TASK_CANCELLED, now, instance_id, TASK_PENDING],
    )?;
    tx.execute(
        "UPDATE flow_instances SET status = ?1, finished_at = ?2, updated_at = ?2 WHERE id = ?3",
        params![STATUS_TERMINATED, now, instance_id],
    )?;
    write_log(
        &tx,
        instance_id,
        Some(actor_id),
        actor_name,
        "终止",
        &instance.current_step_key,
        "",
        reason,
    )?;

    let targets: Vec<i64> = pending
        .into_iter()
        .chain(std::iter::once(instance.initiator_id))
        .collect();
    let detail = if reason.trim().is_empty() { "未填写原因" } else { reason.trim() };
    notify::push_many(&tx, &targets, |_| {
        notify::NewNotification::flow(
            notify::category::FLOW_TERMINATED,
            format!("事项被终止：{}", instance.title),
            format!("{actor_name} 终止：{detail}"),
            instance_id,
        )
    })?;

    tx.commit()?;
    Ok(())
}

struct TaskRecord {
    instance_id: i64,
    assignee_id: i64,
    status: i64,
    step_name: String,
}

fn load_task(conn: &Connection, task_id: i64) -> AppResult<TaskRecord> {
    conn.query_row(
        "SELECT instance_id, assignee_id, status, step_name FROM flow_tasks WHERE id = ?1",
        params![task_id],
        |row| {
            Ok(TaskRecord {
                instance_id: row.get(0)?,
                assignee_id: row.get(1)?,
                status: row.get(2)?,
                step_name: row.get(3)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("任务不存在"))
}

// ---------------- 查询 ----------------

const TASK_SELECT: &str = "SELECT t.id, t.instance_id, i.code, i.title, t.step_key, t.step_name,
                                  t.step_type, t.status, t.assignee_id,
                                  COALESCE(ua.display_name, ''), t.action, t.comment,
                                  t.created_at, t.done_at, COALESCE(ui.display_name, '')
                           FROM flow_tasks t
                           JOIN flow_instances i ON i.id = t.instance_id
                           LEFT JOIN users ua ON ua.id = t.assignee_id
                           LEFT JOIN users ui ON ui.id = i.initiator_id";

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskItem> {
    Ok(TaskItem {
        id: row.get(0)?,
        instance_id: row.get(1)?,
        instance_code: row.get(2)?,
        title: row.get(3)?,
        step_key: row.get(4)?,
        step_name: row.get(5)?,
        step_type: row.get(6)?,
        status: row.get(7)?,
        assignee_id: row.get(8)?,
        assignee_name: row.get(9)?,
        action: row.get(10)?,
        comment: row.get(11)?,
        created_at: row.get(12)?,
        done_at: row.get(13)?,
        initiator_name: row.get(14)?,
    })
}

pub fn my_tasks(conn: &Connection, user_id: i64, done: bool, limit: i64) -> AppResult<Vec<TaskItem>> {
    let sql = format!(
        "{TASK_SELECT} WHERE t.assignee_id = ?1 AND t.status = ?2
         ORDER BY t.id DESC LIMIT ?3"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![user_id, if done { TASK_DONE } else { TASK_PENDING }, limit],
        map_task,
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

const INSTANCE_SELECT: &str = "SELECT i.id, i.code, i.title, COALESCE(d.name, ''), i.status,
                                      i.current_step_key, i.initiator_id,
                                      COALESCE(u.display_name, ''), i.created_at, i.updated_at,
                                      i.finished_at,
                                      (SELECT COUNT(*) FROM flow_tasks t
                                        WHERE t.instance_id = i.id AND t.status = 0)
                               FROM flow_instances i
                               LEFT JOIN flow_defs d ON d.id = i.def_id
                               LEFT JOIN users u ON u.id = i.initiator_id";

pub fn list_instances(
    conn: &Connection,
    scope: &str,
    user_id: i64,
    limit: i64,
) -> AppResult<Vec<InstanceItem>> {
    let (where_clause, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = match scope {
        "created" => ("WHERE i.initiator_id = ?1".into(), vec![Box::new(user_id)]),
        "involved" => (
            "WHERE EXISTS (SELECT 1 FROM flow_tasks t WHERE t.instance_id = i.id AND t.assignee_id = ?1)"
                .into(),
            vec![Box::new(user_id)],
        ),
        _ => (String::new(), vec![]),
    };

    let sql = format!("{INSTANCE_SELECT} {where_clause} ORDER BY i.id DESC LIMIT ?{}", params_vec.len() + 1);

    let mut all: Vec<Box<dyn rusqlite::ToSql>> = params_vec;
    all.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(all.iter()), |row| {
        let step_key: String = row.get(5)?;
        Ok(InstanceItem {
            id: row.get(0)?,
            code: row.get(1)?,
            title: row.get(2)?,
            def_name: row.get(3)?,
            status: row.get(4)?,
            current_step_key: step_key.clone(),
            current_step_name: step_key,
            initiator_id: row.get(6)?,
            initiator_name: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            finished_at: row.get(10)?,
            pending_count: row.get(11)?,
        })
    })?;

    let mut items = rows.collect::<Result<Vec<_>, _>>()?;

    // 步骤名要查配置才知道，逐条补上；列表量不大，代价可以接受
    for item in &mut items {
        if item.status == STATUS_RUNNING {
            if let Ok((_, config)) = load_def(
                conn,
                conn.query_row(
                    "SELECT def_id FROM flow_instances WHERE id = ?1",
                    params![item.id],
                    |row| row.get(0),
                )?,
            ) {
                item.current_step_name = step_display_name(&config, &item.current_step_key);
            }
        } else {
            item.current_step_name = if item.status == STATUS_FINISHED {
                "已完成".into()
            } else {
                "已终止".into()
            };
        }
    }

    Ok(items)
}

pub fn instance_detail(conn: &Connection, instance_id: i64, viewer_id: i64) -> AppResult<InstanceDetail> {
    let instance = load_instance(conn, instance_id)?;
    let (def_name, config) = load_def(conn, instance.def_id)?;

    let (code, title, initiator_name, created_at, updated_at, finished_at): (String, String, String, String, String, Option<String>) =
        conn.query_row(
            "SELECT i.code, i.title, COALESCE(u.display_name, ''), i.created_at, i.updated_at, i.finished_at
             FROM flow_instances i LEFT JOIN users u ON u.id = i.initiator_id WHERE i.id = ?1",
            params![instance_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )?;

    let mut stmt = conn.prepare(&format!("{TASK_SELECT} WHERE t.instance_id = ?1 ORDER BY t.id"))?;
    let tasks = stmt
        .query_map(params![instance_id], map_task)?
        .collect::<Result<Vec<_>, _>>()?;

    let my_pending_task_id = tasks
        .iter()
        .find(|task| task.status == 0 && task.assignee_id == viewer_id && task.step_key == instance.current_step_key)
        .map(|task| task.id);

    let mut stmt = conn.prepare(
        "SELECT id, actor_name, action, from_step, to_step, detail, created_at
         FROM flow_logs WHERE instance_id = ?1 ORDER BY id",
    )?;
    let logs = stmt
        .query_map(params![instance_id], |row| {
            Ok(LogItem {
                id: row.get(0)?,
                actor_name: row.get(1)?,
                action: row.get(2)?,
                from_step: row.get(3)?,
                to_step: row.get(4)?,
                detail: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let step_type = config
        .step(&instance.current_step_key)
        .map(|step| kind_str(step.kind).to_string())
        .unwrap_or_default();

    Ok(InstanceDetail {
        id: instance.id,
        code,
        def_id: instance.def_id,
        def_name,
        title,
        status: instance.status,
        current_step_name: step_display_name(&config, &instance.current_step_key),
        current_step_key: instance.current_step_key,
        current_step_type: step_type,
        initiator_id: instance.initiator_id,
        initiator_name,
        record_id: instance.record_id,
        form: instance.form,
        created_at,
        updated_at,
        finished_at,
        tasks,
        logs,
        my_pending_task_id,
    })
}
