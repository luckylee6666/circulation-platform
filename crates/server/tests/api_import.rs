//! 数据导入相关的端到端测试：字段定义、粘贴/上传、预览、提交、导出。

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::TestApp;

/// 建一套「编号 + 名称 + 数量」的字段定义，编号是必填的唯一键。
async fn setup_fields(app: &TestApp, session: &common::Session) {
    for (code, label, field_type, unique) in [
        ("code", "编号", "text", true),
        ("name", "名称", "text", false),
        ("qty", "数量", "number", false),
    ] {
        let response = app
            .post(
                "/api/fields",
                json!({
                    "code": code,
                    "label": label,
                    "fieldType": field_type,
                    "isUniqueKey": unique,
                    "required": unique,
                }),
                Some(session),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "建字段失败: {:?}", response.body);
    }
}

async fn paste(app: &TestApp, session: &common::Session, text: &str) -> Value {
    let response = app
        .post("/api/imports/paste", json!({ "text": text }), Some(session))
        .await;
    assert_eq!(response.status, StatusCode::OK, "解析失败: {:?}", response.body);
    response.body
}

fn mapping() -> Value {
    json!({ "headerRow": 0, "mapping": { "code": 0, "name": 1, "qty": 2 }, "dedupe": "upsert" })
}

#[tokio::test]
async fn admin_can_manage_field_definitions() {
    let app = TestApp::new();
    let session = app.admin().await;

    let created = app
        .post(
            "/api/fields",
            json!({ "code": "code", "label": "编号", "isUniqueKey": true }),
            Some(&session),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK);

    let list = app.get("/api/fields", Some(&session)).await;
    assert_eq!(list.status, StatusCode::OK);
    let fields = list.body.as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["label"], "编号");
    assert_eq!(fields[0]["isUniqueKey"], true);

    // 非法标识要被挡下来
    let bad = app
        .post(
            "/api/fields",
            json!({ "code": "带中文", "label": "x" }),
            Some(&session),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // 第二个唯一键会把前一个顶掉，保证全局只有一个
    app.post(
        "/api/fields",
        json!({ "code": "other", "label": "另一个", "isUniqueKey": true }),
        Some(&session),
    )
    .await;

    let list = app.get("/api/fields", Some(&session)).await;
    let unique_count = list
        .body
        .as_array()
        .unwrap()
        .iter()
        .filter(|field| field["isUniqueKey"] == true)
        .count();
    assert_eq!(unique_count, 1);
}

#[tokio::test]
async fn paste_import_end_to_end() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let uploaded = paste(
        &app,
        &session,
        "编号\t名称\t数量\nA-1\t螺丝\t10\nA-2\t螺母\t20",
    )
    .await;

    let session_id = uploaded["sessionId"].as_str().unwrap().to_string();
    assert_eq!(uploaded["totalRows"], 3);
    assert_eq!(uploaded["uniqueKeyLabel"], "编号");
    // 表头与字段名一致时应自动配对
    assert_eq!(uploaded["suggestedMapping"]["code"], 0);
    assert_eq!(uploaded["suggestedMapping"]["name"], 1);

    let preview = app
        .post(
            &format!("/api/imports/{session_id}/preview"),
            mapping(),
            Some(&session),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "预览失败: {:?}", preview.body);
    assert_eq!(preview.body["summary"]["total"], 2);
    assert_eq!(preview.body["summary"]["insert"], 2);
    assert_eq!(preview.body["summary"]["invalid"], 0);

    let commit = app
        .post(
            &format!("/api/imports/{session_id}/commit"),
            mapping(),
            Some(&session),
        )
        .await;
    assert_eq!(commit.status, StatusCode::OK, "提交失败: {:?}", commit.body);
    assert_eq!(commit.body["inserted"], 2);
    assert_eq!(commit.body["failed"], 0);

    let records = app.get("/api/records", Some(&session)).await;
    assert_eq!(records.body["total"], 2);

    // 提交后会话就作废了，防止重复提交
    let again = app
        .post(
            &format!("/api/imports/{session_id}/commit"),
            mapping(),
            Some(&session),
        )
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reimporting_the_same_file_updates_instead_of_duplicating() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let text = "编号\t名称\t数量\nA-1\t螺丝\t10";

    let first = paste(&app, &session, text).await;
    app.post(
        &format!("/api/imports/{}/commit", first["sessionId"].as_str().unwrap()),
        mapping(),
        Some(&session),
    )
    .await;

    let second = paste(&app, &session, text).await;
    // 第二次上传应命中上次保存的映射模板
    assert!(second["matchedTemplate"].is_object(), "应复用上次的映射");

    let commit = app
        .post(
            &format!("/api/imports/{}/commit", second["sessionId"].as_str().unwrap()),
            mapping(),
            Some(&session),
        )
        .await;
    assert_eq!(commit.body["inserted"], 0);
    assert_eq!(commit.body["updated"], 1);

    let records = app.get("/api/records", Some(&session)).await;
    assert_eq!(records.body["total"], 1, "重复导入不应产生新记录");
}

#[tokio::test]
async fn csv_upload_handles_gbk_and_reports_bad_rows() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    // 整份文件都是 GBK 编码：编号,名称,数量 / A-1,螺丝,10 / (空),螺母,20
    let text = "编号,名称,数量\nA-1,螺丝,10\n,螺母,20";
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    assert!(
        std::str::from_utf8(&bytes).is_err(),
        "测试数据必须是非法 UTF-8，否则测不出编码识别"
    );

    let uploaded = app.upload("/api/imports/upload", "物料.csv", &bytes, &session).await;
    assert_eq!(uploaded.status, StatusCode::OK, "上传失败: {:?}", uploaded.body);

    let payload = &uploaded.body;
    let session_id = payload["sessionId"].as_str().unwrap().to_string();
    // 表头应为解码后的中文，说明 GBK 被正确识别
    assert_eq!(payload["previewRows"][0][0], "编号");

    let preview = app
        .post(
            &format!("/api/imports/{session_id}/preview"),
            mapping(),
            Some(&session),
        )
        .await;
    assert_eq!(preview.body["summary"]["insert"], 1);
    assert_eq!(preview.body["summary"]["invalid"], 1, "编号为空的那行应判为错误");
}

#[tokio::test]
async fn upsert_requires_a_unique_key_field() {
    let app = TestApp::new();
    let session = app.admin().await;

    // 故意不建唯一键字段
    app.post(
        "/api/fields",
        json!({ "code": "code", "label": "编号" }),
        Some(&session),
    )
    .await;

    let uploaded = paste(&app, &session, "编号\nA-1").await;
    assert!(uploaded["uniqueKeyLabel"].is_null());

    let commit = app
        .post(
            &format!("/api/imports/{}/commit", uploaded["sessionId"].as_str().unwrap()),
            json!({ "headerRow": 0, "mapping": { "code": 0 }, "dedupe": "upsert" }),
            Some(&session),
        )
        .await;

    assert_eq!(commit.status, StatusCode::BAD_REQUEST);
    assert!(
        commit.body["message"].as_str().unwrap().contains("唯一键"),
        "错误提示应说明缺少唯一键：{}",
        commit.body["message"]
    );
}

#[tokio::test]
async fn missing_required_mapping_blocks_commit() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let uploaded = paste(&app, &session, "编号\t名称\t数量\nA-1\t螺丝\t10").await;
    let session_id = uploaded["sessionId"].as_str().unwrap();

    // 只映射了「名称」，必填的「编号」没映射
    let partial = json!({ "headerRow": 0, "mapping": { "name": 1 }, "dedupe": "upsert" });

    let preview = app
        .post(&format!("/api/imports/{session_id}/preview"), partial.clone(), Some(&session))
        .await;
    assert_eq!(preview.status, StatusCode::OK);
    // 唯一键字段是必填之外的额外提示，两条都要有
    assert!(preview.body["mappingErrors"].as_array().unwrap().len() >= 2);

    let commit = app
        .post(&format!("/api/imports/{session_id}/commit"), partial, Some(&session))
        .await;
    assert_eq!(commit.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn export_produces_excel_friendly_csv() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let uploaded = paste(&app, &session, "编号\t名称\t数量\nA-1\t螺,丝\t10").await;
    app.post(
        &format!("/api/imports/{}/commit", uploaded["sessionId"].as_str().unwrap()),
        mapping(),
        Some(&session),
    )
    .await;

    let csv = app.export("/api/records/export", &session).await;

    assert!(csv.starts_with('\u{feff}'), "缺少 BOM，Excel 打开会乱码");
    assert!(csv.contains("编号,名称,数量"), "缺少表头行");
    assert!(csv.contains("\"螺,丝\""), "含逗号的值应被引号包裹");
}

#[tokio::test]
async fn search_only_matches_values_not_field_names() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let uploaded = paste(&app, &session, "编号\t名称\t数量\nA-1\t螺丝\t10").await;
    app.post(
        &format!("/api/imports/{}/commit", uploaded["sessionId"].as_str().unwrap()),
        mapping(),
        Some(&session),
    )
    .await;

    // 「编号」是字段名，不该把记录搜出来
    let by_field_name = app.get("/api/records?keyword=%E7%BC%96%E5%8F%B7", Some(&session)).await;
    assert_eq!(by_field_name.body["total"], 0);

    // 值里含「螺丝」应该搜得到
    let by_value = app.get("/api/records?keyword=%E8%9E%BA%E4%B8%9D", Some(&session)).await;
    assert_eq!(by_value.body["total"], 1);
}

#[tokio::test]
async fn editing_a_record_enforces_required_fields() {
    let app = TestApp::new();
    let session = app.admin().await;
    setup_fields(&app, &session).await;

    let uploaded = paste(&app, &session, "编号\t名称\t数量\nA-1\t螺丝\t10").await;
    app.post(
        &format!("/api/imports/{}/commit", uploaded["sessionId"].as_str().unwrap()),
        mapping(),
        Some(&session),
    )
    .await;

    let records = app.get("/api/records", Some(&session)).await;
    let id = records.body["items"][0]["id"].as_i64().unwrap();

    // 把必填的编号清空应被拒绝
    let cleared = app
        .put(
            &format!("/api/records/{id}"),
            json!({ "data": { "code": "", "name": "螺丝", "qty": 10 } }),
            Some(&session),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::BAD_REQUEST);
    assert!(cleared.body["message"].as_str().unwrap().contains("编号"));

    // 正常修改放行
    let ok = app
        .put(
            &format!("/api/records/{id}"),
            json!({ "data": { "code": "A-1", "name": "螺丝（改）", "qty": 15 } }),
            Some(&session),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);

    let detail = app.get(&format!("/api/records/{id}"), Some(&session)).await;
    assert_eq!(detail.body["data"]["name"], "螺丝（改）");
    assert_eq!(detail.body["data"]["qty"], 15);
}

#[tokio::test]
async fn importing_requires_permission() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let roles = app.get("/api/roles", Some(&admin)).await;
    let viewer_role = roles
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["code"] == "viewer")
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    app.post(
        "/api/users",
        json!({
            "username": "viewer1",
            "displayName": "只读用户",
            "password": "viewer123",
            "roleIds": [viewer_role],
        }),
        Some(&admin),
    )
    .await;

    let viewer = app
        .login("viewer1", "viewer123")
        .await
        .expect("只读用户登录失败");

    let response = app
        .post("/api/imports/paste", json!({ "text": "编号\nA-1" }), Some(&viewer))
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);

    // 但可以读数据（viewer 有 data:view）
    assert_eq!(app.get("/api/records", Some(&viewer)).await.status, StatusCode::OK);
    // 不能改数据（viewer 没有 data:edit）
    let edit = app
        .put("/api/records/1", json!({ "data": {} }), Some(&viewer))
        .await;
    assert_eq!(edit.status, StatusCode::FORBIDDEN);
    // 也不能导出（viewer 没有 data:export）
    let export = app.get("/api/records/export", Some(&viewer)).await;
    assert_eq!(export.status, StatusCode::FORBIDDEN);
}
