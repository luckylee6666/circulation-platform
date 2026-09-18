//! 站内消息。所有提醒都先落到 notifications 表，实时推送只是「让对方立刻知道」，
//! 断线重连后拉一次列表就能补齐，不会丢消息。

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::Serialize;

use crate::error::AppResult;
use crate::util::now_str;

/// 消息分类，前端据此决定图标和是否响铃。
pub mod category {
    /// 有新的事项分派给你
    pub const TASK_ASSIGNED: &str = "task_assigned";
    /// 你的事项被打回
    pub const TASK_REJECTED: &str = "task_rejected";
    /// 你的事项已办结
    pub const FLOW_FINISHED: &str = "flow_finished";
    /// 你的事项被终止
    pub const FLOW_TERMINATED: &str = "flow_terminated";
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: i64,
    pub category: String,
    pub title: String,
    pub content: String,
    pub ref_type: String,
    pub ref_id: Option<i64>,
    pub read_at: Option<String>,
    pub created_at: String,
}

pub struct NewNotification {
    pub category: &'static str,
    pub title: String,
    pub content: String,
    pub ref_type: &'static str,
    pub ref_id: Option<i64>,
}

impl NewNotification {
    pub fn flow(category: &'static str, title: String, content: String, instance_id: i64) -> Self {
        Self {
            category,
            title,
            content,
            ref_type: "flow",
            ref_id: Some(instance_id),
        }
    }
}

pub fn push(conn: &Connection, user_id: i64, item: NewNotification) -> AppResult<()> {
    conn.execute(
        "INSERT INTO notifications (user_id, category, title, content, ref_type, ref_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            user_id,
            item.category,
            item.title,
            item.content,
            item.ref_type,
            item.ref_id,
            now_str()
        ],
    )?;
    Ok(())
}

/// 给多个人发同一条消息，自动去重。
pub fn push_many(
    conn: &Connection,
    user_ids: &[i64],
    build: impl Fn(i64) -> NewNotification,
) -> AppResult<Vec<i64>> {
    let mut seen = std::collections::HashSet::new();
    let mut sent = Vec::new();

    for user_id in user_ids {
        if !seen.insert(*user_id) {
            continue;
        }
        push(conn, *user_id, build(*user_id))?;
        sent.push(*user_id);
    }
    Ok(sent)
}

pub fn list(conn: &Connection, user_id: i64, unread_only: bool, limit: i64) -> AppResult<Vec<Notification>> {
    let sql = format!(
        "SELECT id, category, title, content, ref_type, ref_id, read_at, created_at
         FROM notifications
         WHERE user_id = ?1 {}
         ORDER BY id DESC
         LIMIT ?2",
        if unread_only { "AND read_at IS NULL" } else { "" }
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![user_id, limit], |row| {
        Ok(Notification {
            id: row.get(0)?,
            category: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            ref_type: row.get(4)?,
            ref_id: row.get(5)?,
            read_at: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn unread_count(conn: &Connection, user_id: i64) -> AppResult<i64> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read_at IS NULL",
        params![user_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// ids 为空表示全部标记为已读。
pub fn mark_read(conn: &Connection, user_id: i64, ids: &[i64]) -> AppResult<usize> {
    if ids.is_empty() {
        let affected = conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE user_id = ?2 AND read_at IS NULL",
            params![now_str(), user_id],
        )?;
        return Ok(affected);
    }

    // 只允许标记自己的消息，避免越权改别人的
    let placeholders = std::iter::repeat_n("?", ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "UPDATE notifications SET read_at = ?1
         WHERE user_id = ?2 AND read_at IS NULL AND id IN ({placeholders})"
    );

    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(now_str()),
        Box::new(user_id),
    ];
    for id in ids {
        values.push(Box::new(*id));
    }

    let affected = conn.execute(&sql, params_from_iter(values.iter().map(|value| value.as_ref())))?;
    Ok(affected)
}

/// 未读消息里最新一条的 id，用来判断是否需要提醒（刚开始使用时不该响铃）。
pub fn latest_id(conn: &Connection, user_id: i64) -> AppResult<Option<i64>> {
    let id = conn
        .query_row(
            "SELECT MAX(id) FROM notifications WHERE user_id = ?1",
            params![user_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    Ok(id)
}
