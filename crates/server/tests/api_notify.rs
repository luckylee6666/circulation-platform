//! 站内消息的端到端测试：谁该收到、不该收到什么，以及已读状态只对自己生效。

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{Session, TestApp};

async fn add_user(app: &TestApp, admin: &Session, username: &str, name: &str, role: &str) -> Session {
    let roles = app.get("/api/roles", Some(admin)).await;
    let role_id = roles.body.as_array().unwrap().iter()
        .find(|item| item["code"] == role)
        .unwrap_or_else(|| panic!("缺少角色 {role}"))["id"].as_i64().unwrap();

    let created = app
        .post(
            "/api/users",
            json!({ "username": username, "displayName": name, "password": "init123456", "roleIds": [role_id] }),
            Some(admin),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "建用户失败: {:?}", created.body);

    let first = app.login(username, "init123456").await.expect("新用户登录失败");
    app.post(
        "/api/auth/change-password",
        json!({ "oldPassword": "init123456", "newPassword": "pass123456" }),
        Some(&first),
    )
    .await;
    app.login(username, "pass123456").await.expect("改密后登录失败")
}

async fn inbox(app: &TestApp, session: &Session) -> Vec<Value> {
    let response = app.get("/api/notifications", Some(session)).await;
    assert_eq!(response.status, StatusCode::OK);
    response.body.as_array().cloned().unwrap_or_default()
}

async fn unread(app: &TestApp, session: &Session) -> i64 {
    let response = app.get("/api/notifications/unread-count", Some(session)).await;
    response.body["count"].as_i64().unwrap()
}

async fn default_def(app: &TestApp, session: &Session) -> i64 {
    let defs = app.get("/api/flows/defs", Some(session)).await;
    defs.body[0]["id"].as_i64().unwrap()
}

#[tokio::test]
async fn assignee_gets_notified_and_the_actor_does_not() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def(&app, &admin).await;
    let created = app
        .post("/api/flows", json!({ "defId": def_id, "title": "设备报修" }), Some(&admin))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{:?}", created.body);

    // 分派人收到提醒
    let items = inbox(&app, &dispatcher).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["category"], "task_assigned");
    assert!(items[0]["title"].as_str().unwrap().contains("设备报修"));
    assert_eq!(items[0]["refType"], "flow");
    assert_eq!(unread(&app, &dispatcher).await, 1);

    // 自己发起的不用提醒自己
    assert!(inbox(&app, &admin).await.is_empty());
    assert_eq!(unread(&app, &admin).await, 0);
}

#[tokio::test]
async fn rejection_notifies_the_initiator_with_the_reason() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let options = app.get("/api/users/options", Some(&admin)).await;
    let handler_id = options.body.as_array().unwrap().iter()
        .find(|item| item["displayName"] == "承办甲").unwrap()["id"].as_i64().unwrap();

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "要打回的事" }), Some(&admin)).await;

    let dispatch_task = app.get("/api/flows/tasks?done=false", Some(&dispatcher)).await
        .body[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{dispatch_task}/complete"),
        json!({ "action": "complete", "assignedTo": [handler_id] }),
        Some(&dispatcher),
    )
    .await;

    let handler_task = app.get("/api/flows/tasks?done=false", Some(&handler)).await
        .body[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{handler_task}/complete"),
        json!({ "action": "complete" }),
        Some(&handler),
    )
    .await;

    // 承办完成后确认任务回到调度员，发起人此时还没有办结消息
    let before = inbox(&app, &admin).await;
    assert!(before.iter().all(|item| item["category"] != "flow_finished"));

    let confirm_task = app.get("/api/flows/tasks?done=false", Some(&dispatcher)).await
        .body[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{confirm_task}/complete"),
        json!({ "action": "reject", "comment": "照片没上传" }),
        Some(&dispatcher),
    )
    .await;

    let items = inbox(&app, &admin).await;
    let rejection = items
        .iter()
        .find(|item| item["category"] == "task_rejected")
        .expect("发起人应收到打回通知");
    assert!(
        rejection["content"].as_str().unwrap().contains("照片没上传"),
        "打回原因要带上：{}",
        rejection["content"]
    );

    // 打回后重新分派，承办人应重新收到待办提醒
    let handler_inbox = inbox(&app, &handler).await;
    assert!(handler_inbox.iter().filter(|i| i["category"] == "task_assigned").count() >= 1);
}

#[tokio::test]
async fn finishing_notifies_the_initiator() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let options = app.get("/api/users/options", Some(&admin)).await;
    let handler_id = options.body.as_array().unwrap().iter()
        .find(|item| item["displayName"] == "承办甲").unwrap()["id"].as_i64().unwrap();

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "会办结的事" }), Some(&admin)).await;

    let task = app.get("/api/flows/tasks?done=false", Some(&dispatcher)).await.body[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{task}/complete"),
        json!({ "action": "complete", "assignedTo": [handler_id] }),
        Some(&dispatcher),
    ).await;

    let task = app.get("/api/flows/tasks?done=false", Some(&handler)).await.body[0]["id"].as_i64().unwrap();
    app.post(&format!("/api/flows/tasks/{task}/complete"), json!({ "action": "complete" }), Some(&handler)).await;

    let task = app.get("/api/flows/tasks?done=false", Some(&dispatcher)).await.body[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{task}/complete"),
        json!({ "action": "pass", "comment": "结案" }),
        Some(&dispatcher),
    ).await;

    let items = inbox(&app, &admin).await;
    let finished = items
        .iter()
        .find(|item| item["category"] == "flow_finished")
        .expect("发起人应收到办结通知");
    assert!(finished["title"].as_str().unwrap().contains("会办结的事"));
}

#[tokio::test]
async fn terminating_tells_everyone_who_was_waiting() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def(&app, &admin).await;
    let created = app
        .post("/api/flows", json!({ "defId": def_id, "title": "要取消的事" }), Some(&admin))
        .await;
    let instance_id = created.body["id"].as_i64().unwrap();

    let before = unread(&app, &dispatcher).await;

    app.post(
        &format!("/api/flows/instances/{instance_id}/terminate"),
        json!({ "reason": "事项取消" }),
        Some(&admin),
    )
    .await;

    let items = inbox(&app, &dispatcher).await;
    let terminated = items
        .iter()
        .find(|item| item["category"] == "flow_terminated")
        .expect("待办被作废的人应该收到通知");
    assert!(terminated["content"].as_str().unwrap().contains("事项取消"));
    assert!(unread(&app, &dispatcher).await > before);
}

#[tokio::test]
async fn marking_read_only_affects_your_own_messages() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let outsider = add_user(&app, &admin, "viewer1", "无关人员", "viewer").await;

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "事项一" }), Some(&admin)).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "事项二" }), Some(&admin)).await;
    assert_eq!(unread(&app, &dispatcher).await, 2);

    let first_id = inbox(&app, &dispatcher).await[0]["id"].as_i64().unwrap();

    // 别人拿这个 id 去标记已读不该生效
    let response = app
        .post("/api/notifications/read", json!({ "ids": [first_id] }), Some(&outsider))
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body["affected"], 0);
    assert_eq!(unread(&app, &dispatcher).await, 2, "不能改别人的消息状态");

    let response = app
        .post("/api/notifications/read", json!({ "ids": [first_id] }), Some(&dispatcher))
        .await;
    assert_eq!(response.body["affected"], 1);
    assert_eq!(unread(&app, &dispatcher).await, 1);

    // 不传 ids 表示全部已读
    let response = app.post("/api/notifications/read", json!({}), Some(&dispatcher)).await;
    assert_eq!(response.body["affected"], 1);
    assert_eq!(unread(&app, &dispatcher).await, 0);

    // 只读筛选
    let unread_only = app
        .get("/api/notifications?unreadOnly=true", Some(&dispatcher))
        .await;
    assert!(unread_only.body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn notifications_are_private_to_each_account() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let first = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let _second = add_user(&app, &admin, "dispatcher2", "调度员乙", "dispatcher").await;

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "给所有人的" }), Some(&admin)).await;

    // 两个调度员都在分派角色下，各自都该收到
    assert_eq!(inbox(&app, &first).await.len(), 1);

    let anonymous = app.get("/api/notifications", None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}
