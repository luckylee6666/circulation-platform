//! 统计报表。

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::db;
use crate::domain::stats;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats/report", get(report))
        .route("/stats/mine", get(mine))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionsQuery {
    days: Option<i64>,
    overdue_hours: Option<i64>,
}

impl OptionsQuery {
    fn into_options(self) -> stats::Options {
        stats::Options::new(self.days, self.overdue_hours)
    }
}

async fn report(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<OptionsQuery>,
) -> AppResult<Json<stats::Report>> {
    current.require("stats:view")?;
    let options = query.into_options();

    let report = db::run(state.pool.clone(), move |conn| stats::report(conn, &options)).await?;
    Ok(Json(report))
}

/// 个人视角的统计，任何登录用户都能看自己的。
async fn mine(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<OptionsQuery>,
) -> AppResult<Json<stats::Mine>> {
    let options = query.into_options();
    let user_id = current.id;

    let mine = db::run(state.pool.clone(), move |conn| {
        stats::mine(conn, user_id, &options)
    })
    .await?;

    Ok(Json(mine))
}
