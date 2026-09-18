//! 流转统计。
//!
//! 时间统一在 Rust 侧算好边界再当字符串比较，不用 SQL 的 `julianday('now')`——
//! 库里的时间是本地时间，而 SQLite 的 `now` 是 UTC，混用会差出一个时区。
//! 时长换算仍然用 `julianday`，因为两边都是同一个库里的本地时间，差值不受影响。

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::error::AppResult;
use crate::util::{date_offset_str, datetime_before_hours, now_str, today_str};

const DEFAULT_TREND_DAYS: i64 = 30;
const DEFAULT_OVERDUE_HOURS: i64 = 48;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub total: i64,
    pub running: i64,
    pub finished: i64,
    pub terminated: i64,
    pub created_today: i64,
    pub finished_today: i64,
    /// 当前环节停留超过阈值的条数
    pub overdue: i64,
    pub overdue_hours: i64,
    /// 已办结事项的平均耗时（小时）
    pub avg_finish_hours: Option<f64>,
    /// 在办事项当前平均已耗时（小时）
    pub avg_running_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepStat {
    pub step_key: String,
    pub step_name: String,
    pub step_type: String,
    pub total: i64,
    pub pending: i64,
    pub done: i64,
    pub avg_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssigneeStat {
    pub user_id: i64,
    pub display_name: String,
    pub dept: String,
    pub pending: i64,
    pub done: i64,
    pub overdue: i64,
    pub avg_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub day: String,
    pub created: i64,
    pub finished: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSummary {
    pub records: i64,
    pub batches: i64,
    pub last_import_at: Option<String>,
    pub last_import_rows: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub overview: Overview,
    pub steps: Vec<StepStat>,
    pub assignees: Vec<AssigneeStat>,
    pub trend: Vec<TrendPoint>,
    pub data: DataSummary,
}

/// 个人视角的统计，不需要全局统计权限。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mine {
    pub pending: i64,
    pub done: i64,
    pub created: i64,
    pub overdue: i64,
    pub overdue_hours: i64,
    pub avg_hours: Option<f64>,
    pub trend: Vec<TrendPoint>,
}

pub struct Options {
    pub trend_days: i64,
    pub overdue_hours: i64,
}

impl Options {
    pub fn new(trend_days: Option<i64>, overdue_hours: Option<i64>) -> Self {
        Self {
            trend_days: trend_days.unwrap_or(DEFAULT_TREND_DAYS).clamp(7, 180),
            overdue_hours: overdue_hours.unwrap_or(DEFAULT_OVERDUE_HOURS).clamp(1, 24 * 30),
        }
    }
}

pub fn report(conn: &Connection, options: &Options) -> AppResult<Report> {
    Ok(Report {
        overview: overview(conn, options)?,
        steps: step_stats(conn)?,
        assignees: assignee_stats(conn, options)?,
        trend: trend(conn, options.trend_days, None)?,
        data: data_summary(conn)?,
    })
}

pub fn mine(conn: &Connection, user_id: i64, options: &Options) -> AppResult<Mine> {
    let pending = count(
        conn,
        "SELECT COUNT(*) FROM flow_tasks t JOIN flow_instances i ON i.id = t.instance_id
         WHERE t.assignee_id = ?1 AND t.status = 0 AND i.status = 1",
        &[&user_id],
    )?;

    let done = count(
        conn,
        "SELECT COUNT(*) FROM flow_tasks WHERE assignee_id = ?1 AND status = 1",
        &[&user_id],
    )?;

    let created = count(
        conn,
        "SELECT COUNT(*) FROM flow_instances WHERE initiator_id = ?1",
        &[&user_id],
    )?;

    let cutoff = datetime_before_hours(options.overdue_hours);
    let overdue = count(
        conn,
        "SELECT COUNT(*) FROM flow_tasks t JOIN flow_instances i ON i.id = t.instance_id
         WHERE t.assignee_id = ?1 AND t.status = 0 AND i.status = 1 AND t.created_at < ?2",
        &[&user_id, &cutoff],
    )?;

    let avg_hours = conn.query_row(
        "SELECT AVG((julianday(done_at) - julianday(created_at)) * 24)
         FROM flow_tasks
         WHERE assignee_id = ?1 AND status = 1 AND done_at IS NOT NULL",
        params![user_id],
        |row| row.get::<_, Option<f64>>(0),
    )?;

    Ok(Mine {
        pending,
        done,
        created,
        overdue,
        overdue_hours: options.overdue_hours,
        avg_hours,
        trend: trend(conn, options.trend_days, Some(user_id))?,
    })
}

fn overview(conn: &Connection, options: &Options) -> AppResult<Overview> {
    let today = today_str();
    let cutoff = datetime_before_hours(options.overdue_hours);

    let (total, running, finished, terminated, created_today, finished_today) = conn.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN status = 1 THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 2 THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 3 THEN 1 ELSE 0 END),
                SUM(CASE WHEN substr(created_at, 1, 10) = ?1 THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 2 AND substr(finished_at, 1, 10) = ?1 THEN 1 ELSE 0 END)
         FROM flow_instances",
        params![today],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            ))
        },
    )?;

    // 超期看的是「当前环节的待办挂了多久」，而不是流程创建了多久
    let overdue = count(
        conn,
        "SELECT COUNT(DISTINCT i.id)
         FROM flow_instances i
         JOIN flow_tasks t ON t.instance_id = i.id AND t.status = 0
         WHERE i.status = 1 AND t.created_at < ?1",
        &[&cutoff],
    )?;

    let avg_finish_hours = conn.query_row(
        "SELECT AVG((julianday(finished_at) - julianday(created_at)) * 24)
         FROM flow_instances
         WHERE status = 2 AND finished_at IS NOT NULL",
        [],
        |row| row.get::<_, Option<f64>>(0),
    )?;

    let avg_running_hours = conn.query_row(
        "SELECT AVG((julianday(?1) - julianday(created_at)) * 24)
         FROM flow_instances WHERE status = 1",
        params![now_str()],
        |row| row.get::<_, Option<f64>>(0),
    )?;

    Ok(Overview {
        total,
        running,
        finished,
        terminated,
        created_today,
        finished_today,
        overdue,
        overdue_hours: options.overdue_hours,
        avg_finish_hours,
        avg_running_hours,
    })
}

fn step_stats(conn: &Connection) -> AppResult<Vec<StepStat>> {
    let mut stmt = conn.prepare(
        "SELECT step_key, step_name, step_type,
                COUNT(*),
                SUM(CASE WHEN status = 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 1 THEN 1 ELSE 0 END),
                AVG(CASE WHEN done_at IS NOT NULL
                         THEN (julianday(done_at) - julianday(created_at)) * 24 END)
         FROM flow_tasks
         GROUP BY step_key, step_name, step_type
         ORDER BY COUNT(*) DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(StepStat {
            step_key: row.get(0)?,
            step_name: row.get(1)?,
            step_type: row.get(2)?,
            total: row.get(3)?,
            pending: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            done: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            avg_hours: row.get(6)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn assignee_stats(conn: &Connection, options: &Options) -> AppResult<Vec<AssigneeStat>> {
    let cutoff = datetime_before_hours(options.overdue_hours);

    let mut stmt = conn.prepare(
        "SELECT u.id, u.display_name, u.dept,
                SUM(CASE WHEN t.status = 0 THEN 1 ELSE 0 END),
                SUM(CASE WHEN t.status = 1 THEN 1 ELSE 0 END),
                SUM(CASE WHEN t.status = 0 AND t.created_at < ?1 THEN 1 ELSE 0 END),
                AVG(CASE WHEN t.done_at IS NOT NULL
                         THEN (julianday(t.done_at) - julianday(t.created_at)) * 24 END)
         FROM flow_tasks t
         JOIN users u ON u.id = t.assignee_id
         GROUP BY u.id, u.display_name, u.dept
         ORDER BY 4 DESC, 5 DESC",
    )?;

    let rows = stmt.query_map(params![cutoff], |row| {
        Ok(AssigneeStat {
            user_id: row.get(0)?,
            display_name: row.get(1)?,
            dept: row.get(2)?,
            pending: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            done: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            overdue: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            avg_hours: row.get(6)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 按天统计新增与办结。没有数据的日期也要补零，否则折线图会断。
fn trend(conn: &Connection, days: i64, user_id: Option<i64>) -> AppResult<Vec<TrendPoint>> {
    let from = date_offset_str(-(days - 1));
    let user_filter = if user_id.is_some() {
        "AND initiator_id = ?2"
    } else {
        ""
    };

    let sql = format!(
        "SELECT day, SUM(created), SUM(finished) FROM (
             SELECT substr(created_at, 1, 10) AS day, 1 AS created, 0 AS finished
             FROM flow_instances WHERE substr(created_at, 1, 10) >= ?1 {user_filter}
             UNION ALL
             SELECT substr(finished_at, 1, 10) AS day, 0, 1
             FROM flow_instances
             WHERE status = 2 AND finished_at IS NOT NULL AND substr(finished_at, 1, 10) >= ?1 {user_filter}
         )
         GROUP BY day"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut by_day: HashMap<String, (i64, i64)> = HashMap::new();

    let mut rows = match user_id {
        Some(id) => stmt.query(params![from, id])?,
        None => stmt.query(params![from])?,
    };

    while let Some(row) = rows.next()? {
        let day: String = row.get(0)?;
        let created: i64 = row.get::<_, Option<i64>>(1)?.unwrap_or(0);
        let finished: i64 = row.get::<_, Option<i64>>(2)?.unwrap_or(0);
        by_day.insert(day, (created, finished));
    }

    let points = (0..days)
        .map(|offset| {
            let day = date_offset_str(-(days - 1 - offset));
            let (created, finished) = by_day.get(&day).copied().unwrap_or((0, 0));
            TrendPoint {
                day,
                created,
                finished,
            }
        })
        .collect();

    Ok(points)
}

fn data_summary(conn: &Connection) -> AppResult<DataSummary> {
    let records = count(conn, "SELECT COUNT(*) FROM records", &[])?;
    let batches = count(conn, "SELECT COUNT(*) FROM import_batches", &[])?;

    let last = conn
        .query_row(
            "SELECT created_at, inserted + updated FROM import_batches ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;

    Ok(DataSummary {
        records,
        batches,
        last_import_at: last.as_ref().map(|(at, _)| at.clone()),
        last_import_rows: last.map(|(_, rows)| rows).unwrap_or(0),
    })
}

fn count(conn: &Connection, sql: &str, values: &[&dyn rusqlite::ToSql]) -> AppResult<i64> {
    let value = conn.query_row(sql, values, |row| row.get(0))?;
    Ok(value)
}
