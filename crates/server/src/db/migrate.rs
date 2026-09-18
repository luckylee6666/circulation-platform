use rusqlite::{Connection, OptionalExtension};

use crate::error::AppResult;
use crate::util::now_str;

/// 按顺序执行的迁移。已发布的迁移不要修改，新增改动请追加新文件。
const MIGRATIONS: &[(&str, &str)] = &[("0001_init", include_str!("migrations/0001_init.sql"))];

pub fn run(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    TEXT PRIMARY KEY,
             applied_at TEXT NOT NULL
         );",
    )?;

    for (version, sql) in MIGRATIONS {
        let applied = conn
            .query_row("SELECT 1 FROM schema_migrations WHERE version = ?1", [version], |_| Ok(()))
            .optional()?
            .is_some();
        if applied {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now_str()],
        )?;
        tx.commit()?;

        tracing::info!("已应用数据库迁移 {version}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, MIGRATIONS.len() as i64);
    }
}
