use std::path::Path;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::error::{AppError, AppResult};

pub mod migrate;

pub type Pool = r2d2::Pool<SqliteConnectionManager>;
pub type PooledConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// 打开（或创建）数据库连接池。
pub fn init_pool(db_path: &Path) -> AppResult<Pool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // journal_mode 是写进数据库文件的持久设置，只需设一次。
    // 若放进每连接的 init 里，池并发建连时会互相抢排他锁，冒出 database is locked。
    {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    }

    let manager = SqliteConnectionManager::file(db_path).with_init(|conn: &mut Connection| {
        conn.execute_batch(
            "PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;",
        )
    });

    let pool = r2d2::Pool::builder()
        .max_size(8)
        .min_idle(Some(2))
        .build(manager)?;
    Ok(pool)
}

/// 在阻塞线程池里执行数据库操作，避免同步 IO 卡住 async 运行时。
pub async fn run<T, F>(pool: Pool, f: F) -> AppResult<T>
where
    F: FnOnce(&mut PooledConn) -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut conn = pool.get()?;
        f(&mut conn)
    })
    .await
    .map_err(|err| AppError::Internal(format!("数据库任务异常: {err}")))?
}
