use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::error::AppResult;
use crate::state::AppState;

pub mod auth;
pub mod fields;
pub mod flows;
pub mod imports;
pub mod notifications;
pub mod records;
pub mod roles;
pub mod site;
pub mod stats;
pub mod users;

pub fn build(state: AppState) -> AppResult<Router> {
    let api = Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(roles::routes())
        .merge(site::routes())
        .merge(fields::routes())
        .merge(imports::routes())
        .merge(records::routes())
        .merge(flows::routes())
        .merge(notifications::routes())
        .merge(stats::routes());

    Ok(Router::new()
        .nest("/api", api)
        .fallback(crate::web_assets::serve)
        .with_state(state))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "name": "circulation-platform",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
