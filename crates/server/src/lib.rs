//! 流转平台服务端：HTTP API + 内嵌 Web 资源 + SQLite 存储。
//!
//! 这个 crate 刻意不依赖 Tauri，因此可以独立运行（`cargo run -p circulation-server`），
//! 桌面端只是它的宿主进程。

pub mod auth;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod extract;
pub mod routes;
pub mod seed;
pub mod state;
pub mod util;
pub mod web_assets;

use axum::Router;

use crate::config::Config;
use crate::error::AppResult;
use crate::state::AppState;

/// 打开数据库、跑迁移、写入种子数据，返回可用的应用状态。
pub fn bootstrap(config: Config) -> AppResult<AppState> {
    let pool = db::init_pool(&config.db_path())?;
    {
        let mut conn = pool.get()?;
        db::migrate::run(&mut conn)?;
        seed::apply(&mut conn)?;
    }
    Ok(AppState::new(config, pool))
}

/// 构建完整的 HTTP 应用（API + 静态资源）。
pub fn build_app(state: AppState) -> AppResult<Router> {
    routes::build(state)
}
