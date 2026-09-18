//! 平台名称的读写与校验。
//!
//! 名称会出现在登录页上，所以读取接口必须是公开的——这条要专门钉住，
//! 免得以后有人顺手给它加上登录校验，把登录页搞成空白标题。

mod common;

use axum::http::StatusCode;
use serde_json::json;

use common::TestApp;

#[tokio::test]
async fn anyone_can_read_the_site_name() {
    let app = TestApp::new();

    // 不带任何登录凭证
    let response = app.get("/api/site", None).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["name"], "流转平台");
}

#[tokio::test]
async fn writing_requires_login() {
    let app = TestApp::new();

    let response = app.put("/api/site", json!({ "name": "某某办公平台" }), None).await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_can_rename_and_read_back() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let updated = app
        .put("/api/site", json!({ "name": "某某办公平台" }), Some(&admin))
        .await;
    assert_eq!(updated.status, StatusCode::OK);
    assert_eq!(updated.body["name"], "某某办公平台");

    // 公开读接口也要跟着变
    let read = app.get("/api/site", None).await;
    assert_eq!(read.body["name"], "某某办公平台");
}

#[tokio::test]
async fn name_is_trimmed_before_saving() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let response = app
        .put("/api/site", json!({ "name": "  某某办公平台  " }), Some(&admin))
        .await;

    assert_eq!(response.body["name"], "某某办公平台");
}

#[tokio::test]
async fn blank_name_is_rejected() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let response = app.put("/api/site", json!({ "name": "   " }), Some(&admin)).await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn overlong_name_is_rejected() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let too_long = "长".repeat(25);
    let response = app
        .put("/api/site", json!({ "name": too_long }), Some(&admin))
        .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn user_without_settings_permission_cannot_rename() {
    let app = TestApp::new();
    let admin = app.admin().await;

    // 造一个只有只读权限的角色和账号
    let roles = app.get("/api/roles", Some(&admin)).await;
    let viewer_id = roles.body.as_array().unwrap().iter()
        .find(|role| role["code"] == "viewer")
        .expect("没有 viewer 角色")["id"]
        .as_i64()
        .unwrap();

    app.post(
        "/api/users",
        json!({
            "username": "reader",
            "displayName": "只读用户",
            "password": "reader123456",
            "roleIds": [viewer_id],
        }),
        Some(&admin),
    )
    .await;

    let reader = app.login("reader", "reader123456").await.expect("只读用户登录失败");
    // 首次登录要改密码，改完拿到新会话
    let reader = app
        .post(
            "/api/auth/change-password",
            json!({ "oldPassword": "reader123456", "newPassword": "reader654321" }),
            Some(&reader),
        )
        .await;
    assert_eq!(reader.status, StatusCode::OK);
    let reader = app.login("reader", "reader654321").await.expect("改密后登录失败");

    let response = app
        .put("/api/site", json!({ "name": "冒充改名" }), Some(&reader))
        .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
}
