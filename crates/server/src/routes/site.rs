use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::permission::AuditEntry;
use crate::auth::CurrentUser;
use crate::db;
use crate::domain::setting::{self, SiteBranding};
use crate::error::AppResult;
use crate::extract::ClientIp;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // 登录页也要显示平台名称，所以这个接口不能要求登录
        .route("/site", get(read_site).put(update_site))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteInput {
    pub name: String,
}

async fn read_site(State(state): State<AppState>) -> AppResult<Json<SiteBranding>> {
    let branding = db::run(state.pool.clone(), |conn| setting::branding(conn)).await?;
    Ok(Json(branding))
}

async fn update_site(
    State(state): State<AppState>,
    current: CurrentUser,
    ClientIp(ip): ClientIp,
    Json(input): Json<SiteInput>,
) -> AppResult<Json<SiteBranding>> {
    current.require("settings:manage")?;
    let name = setting::normalize_site_name(&input.name)?;

    let branding = db::run(state.pool.clone(), move |conn| {
        setting::set(conn, setting::KEY_SITE_NAME, &name)?;
        AuditEntry {
            actor_id: Some(current.id),
            actor_name: &current.display_name,
            action: "修改平台名称",
            target_type: "setting",
            target_id: setting::KEY_SITE_NAME,
            detail: &name,
            ip: &ip,
        }
        .record(conn);
        setting::branding(conn)
    })
    .await?;

    state.publish("setting");
    Ok(Json(branding))
}
