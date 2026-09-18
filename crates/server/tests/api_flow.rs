//! 流转全流程的端到端测试：发起 → 分派 → 承办 → 确认，以及打回、权限与统计口径。

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{Session, TestApp};

/// 建一个用户并返回登录会话，用来扮演流程里的不同角色。
async fn add_user(
    app: &TestApp,
    admin: &Session,
    username: &str,
    display_name: &str,
    role_code: &str,
) -> Session {
    let roles = app.get("/api/roles", Some(admin)).await;
    let role_id = roles
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|role| role["code"] == role_code)
        .unwrap_or_else(|| panic!("缺少角色 {role_code}"))["id"]
        .as_i64()
        .unwrap();

    let created = app
        .post(
            "/api/users",
            json!({
                "username": username,
                "displayName": display_name,
                "password": "init123456",
                "roleIds": [role_id],
            }),
            Some(admin),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "建用户失败: {:?}", created.body);

    // 新用户首登要改密，先改掉才能正常用
    let first = app.login(username, "init123456").await.expect("新用户登录失败");
    app.post(
        "/api/auth/change-password",
        json!({ "oldPassword": "init123456", "newPassword": "pass123456" }),
        Some(&first),
    )
    .await;

    app.login(username, "pass123456")
        .await
        .expect("改密后登录失败")
}

async fn default_def_id(app: &TestApp, session: &Session) -> i64 {
    let defs = app.get("/api/flows/defs", Some(session)).await;
    assert_eq!(defs.status, StatusCode::OK);
    let list = defs.body.as_array().unwrap();
    assert_eq!(list.len(), 1, "初始化应写入一份默认流程模板");
    assert_eq!(list[0]["code"], "default");
    list[0]["id"].as_i64().unwrap()
}

async fn my_todo(app: &TestApp, session: &Session) -> Vec<Value> {
    let response = app.get("/api/flows/tasks?done=false", Some(session)).await;
    assert_eq!(response.status, StatusCode::OK);
    response.body.as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn default_flow_definition_is_seeded_and_valid() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let defs = app.get("/api/flows/defs", Some(&admin)).await;
    let config = &defs.body[0]["config"];

    let steps = config["steps"].as_array().unwrap();
    let kinds: Vec<&str> = steps.iter().map(|step| step["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["start", "dispatch", "handle", "confirm"]);

    // 承办步骤应当由分派时选中的人来做
    let handle = steps.iter().find(|step| step["kind"] == "handle").unwrap();
    assert_eq!(handle["assignee"]["mode"], "assigned");
}

#[tokio::test]
async fn full_circulation_round_trip() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler_a = add_user(&app, &admin, "handler1", "承办甲", "handler").await;
    let handler_b = add_user(&app, &admin, "handler2", "承办乙", "handler").await;

    // 用户 id 用于分派
    let options = app.get("/api/users/options", Some(&admin)).await;
    let id_of = |name: &str| {
        options
            .body
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["displayName"] == name)
            .unwrap_or_else(|| panic!("找不到用户 {name}"))["id"]
            .as_i64()
            .unwrap()
    };

    let def_id = default_def_id(&app, &admin).await;

    // 1) 发起：默认流程里发起人是管理员，第一步任务应落到调度员身上
    let created = app
        .post(
            "/api/flows",
            json!({ "defId": def_id, "title": "三月份设备报修", "form": { "urgency": "普通" } }),
            Some(&admin),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK, "发起失败: {:?}", created.body);
    let instance_id = created.body["id"].as_i64().unwrap();

    let todo = my_todo(&app, &dispatcher).await;
    assert_eq!(todo.len(), 1, "调度员应收到分派任务");
    assert_eq!(todo[0]["stepName"], "分派");
    let dispatch_task = todo[0]["id"].as_i64().unwrap();
    assert_eq!(todo[0]["initiatorName"], "系统管理员");

    // 发起人自己不该有待办（发起步骤是自动通过的）
    assert!(my_todo(&app, &admin).await.is_empty());

    // 2) 分派给两个承办人
    let dispatched = app
        .post(
            &format!("/api/flows/tasks/{dispatch_task}/complete"),
            json!({
                "action": "complete",
                "comment": "请两位一起看一下",
                "assignedTo": [id_of("承办甲"), id_of("承办乙")],
            }),
            Some(&dispatcher),
        )
        .await;
    assert_eq!(dispatched.status, StatusCode::OK, "分派失败: {:?}", dispatched.body);

    // 3) 两个承办人都收到任务
    for handler in [&handler_a, &handler_b] {
        let todo = my_todo(&app, handler).await;
        assert_eq!(todo.len(), 1);
        assert_eq!(todo[0]["stepName"], "承办");
    }

    // 全部完成规则下，只有一个人做完时流程不能往下走
    let first_task = my_todo(&app, &handler_a).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{first_task}/complete"),
        json!({ "action": "complete", "comment": "已处理" }),
        Some(&handler_a),
    )
    .await;

    let detail = app.get(&format!("/api/flows/instances/{instance_id}"), Some(&admin)).await;
    assert_eq!(detail.body["currentStepKey"], "handle", "还有人没完成，不应推进");
    assert_eq!(detail.body["status"], 1);

    // 停在原地等其他人时，时间线不能写成「结束」——那会让人以为流程已经办结
    let last_log = detail.body["logs"].as_array().unwrap().last().unwrap();
    assert_eq!(last_log["action"], "承办完成");
    assert_eq!(last_log["toStep"], "", "等待其他人时不应记录流转目标");

    // 4) 第二个人也完成后进入确认
    let second_task = my_todo(&app, &handler_b).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{second_task}/complete"),
        json!({ "action": "complete", "comment": "已处理" }),
        Some(&handler_b),
    )
    .await;

    let detail = app.get(&format!("/api/flows/instances/{instance_id}"), Some(&admin)).await;
    assert_eq!(detail.body["currentStepKey"], "confirm");

    // 5) 确认打回，退回分派步骤
    let confirm_task = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();
    let rejected = app
        .post(
            &format!("/api/flows/tasks/{confirm_task}/complete"),
            json!({ "action": "reject", "comment": "处理得不对，重做" }),
            Some(&dispatcher),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::OK, "打回失败: {:?}", rejected.body);

    let detail = app.get(&format!("/api/flows/instances/{instance_id}"), Some(&admin)).await;
    assert_eq!(detail.body["currentStepKey"], "dispatch", "打回应退回分派步骤");
    assert_eq!(detail.body["status"], 1, "打回后流程仍在进行中");

    // 打回后调度员重新拿到分派任务
    assert_eq!(my_todo(&app, &dispatcher).await.len(), 1);

    // 6) 重新分派后确认通过，流程结束
    let dispatch_task = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{dispatch_task}/complete"),
        json!({ "action": "complete", "assignedTo": [id_of("承办甲")] }),
        Some(&dispatcher),
    )
    .await;

    let handler_task = my_todo(&app, &handler_a).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{handler_task}/complete"),
        json!({ "action": "complete" }),
        Some(&handler_a),
    )
    .await;

    let confirm_task = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{confirm_task}/complete"),
        json!({ "action": "pass", "comment": "通过" }),
        Some(&dispatcher),
    )
    .await;

    let detail = app.get(&format!("/api/flows/instances/{instance_id}"), Some(&admin)).await;
    assert_eq!(detail.body["status"], 2, "确认通过后流程应结束");
    assert!(detail.body["finishedAt"].is_string());

    // 时间线应完整记录每一步
    let logs = detail.body["logs"].as_array().unwrap();
    let actions: Vec<&str> = logs.iter().map(|log| log["action"].as_str().unwrap()).collect();
    assert!(actions.contains(&"发起"));
    assert!(actions.contains(&"分派"));
    assert!(actions.contains(&"打回"));
    assert!(actions.contains(&"确认通过"));

    // 所有人都没有待办了
    for session in [&admin, &dispatcher, &handler_a, &handler_b] {
        assert!(my_todo(&app, session).await.is_empty());
    }
}

#[tokio::test]
async fn only_the_assignee_can_complete_a_task() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let other = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let def_id = default_def_id(&app, &admin).await;
    app.post(
        "/api/flows",
        json!({ "defId": def_id, "title": "测试事项" }),
        Some(&admin),
    )
    .await;

    let task_id = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();

    let response = app
        .post(
            &format!("/api/flows/tasks/{task_id}/complete"),
            json!({ "action": "complete", "assignedTo": [] }),
            Some(&other),
        )
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dispatch_without_targets_is_rejected_with_a_clear_message() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def_id(&app, &admin).await;
    app.post(
        "/api/flows",
        json!({ "defId": def_id, "title": "测试事项" }),
        Some(&admin),
    )
    .await;

    let task_id = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();

    let response = app
        .post(
            &format!("/api/flows/tasks/{task_id}/complete"),
            json!({ "action": "complete", "assignedTo": [] }),
            Some(&dispatcher),
        )
        .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(
        response.body["message"].as_str().unwrap().contains("指派"),
        "提示应说明要先选人：{}",
        response.body["message"]
    );
}

#[tokio::test]
async fn a_task_cannot_be_completed_twice() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let options = app.get("/api/users/options", Some(&admin)).await;
    let handler_id = options.body.as_array().unwrap().iter()
        .find(|item| item["displayName"] == "承办甲")
        .unwrap()["id"].as_i64().unwrap();

    let def_id = default_def_id(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "测试" }), Some(&admin)).await;

    let task_id = my_todo(&app, &dispatcher).await[0]["id"].as_i64().unwrap();
    app.post(
        &format!("/api/flows/tasks/{task_id}/complete"),
        json!({ "action": "complete", "assignedTo": [handler_id] }),
        Some(&dispatcher),
    )
    .await;

    let again = app
        .post(
            &format!("/api/flows/tasks/{task_id}/complete"),
            json!({ "action": "complete" }),
            Some(&dispatcher),
        )
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    assert!(again.body["message"].as_str().unwrap().contains("已经处理"));

    let _ = handler;
}

#[tokio::test]
async fn instance_lists_are_scoped_to_the_viewer() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let outsider = add_user(&app, &admin, "outsider1", "无关人员", "viewer").await;

    let def_id = default_def_id(&app, &admin).await;
    app.post(
        "/api/flows",
        json!({ "defId": def_id, "title": "只有调度员相关" }),
        Some(&admin),
    )
    .await;

    let mine = app.get("/api/flows/instances?scope=created", Some(&admin)).await;
    assert_eq!(mine.body.as_array().unwrap().len(), 1);

    let involved = app.get("/api/flows/instances?scope=involved", Some(&dispatcher)).await;
    assert_eq!(involved.body.as_array().unwrap().len(), 1, "承办人应能看到参与的流程");

    let not_mine = app.get("/api/flows/instances?scope=created", Some(&outsider)).await;
    assert!(not_mine.body.as_array().unwrap().is_empty());

    // 「全部」需要更高权限
    let all = app.get("/api/flows/instances?scope=all", Some(&outsider)).await;
    assert_eq!(all.status, StatusCode::FORBIDDEN);

    let all = app.get("/api/flows/instances?scope=all", Some(&admin)).await;
    assert_eq!(all.status, StatusCode::OK);
}

#[tokio::test]
async fn terminating_cancels_pending_tasks() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def_id(&app, &admin).await;
    let created = app
        .post("/api/flows", json!({ "defId": def_id, "title": "要终止的" }), Some(&admin))
        .await;
    let instance_id = created.body["id"].as_i64().unwrap();

    assert_eq!(my_todo(&app, &dispatcher).await.len(), 1);

    let terminated = app
        .post(
            &format!("/api/flows/instances/{instance_id}/terminate"),
            json!({ "reason": "事项取消" }),
            Some(&admin),
        )
        .await;
    assert_eq!(terminated.status, StatusCode::OK);

    assert!(my_todo(&app, &dispatcher).await.is_empty(), "终止后待办应作废");

    let detail = app.get(&format!("/api/flows/instances/{instance_id}"), Some(&admin)).await;
    assert_eq!(detail.body["status"], 3);
}

#[tokio::test]
async fn flow_definition_editing_is_permission_guarded() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let viewer = add_user(&app, &admin, "viewer1", "只读用户", "viewer").await;

    let def_id = default_def_id(&app, &admin).await;

    // 只读用户不能改流程定义
    let denied = app
        .put(
            &format!("/api/flows/defs/{def_id}"),
            json!({
                "code": "default",
                "name": "改个名",
                "config": { "steps": [], "rules": [] }
            }),
            Some(&viewer),
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    // 管理员提交非法配置会被挡下来
    let invalid = app
        .put(
            &format!("/api/flows/defs/{def_id}"),
            json!({
                "code": "default",
                "name": "缺发起步骤",
                "config": {
                    "steps": [
                        { "key": "a", "name": "承办", "kind": "handle", "assignee": { "mode": "initiator" } }
                    ],
                    "rules": []
                }
            }),
            Some(&admin),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert!(invalid.body["message"].as_str().unwrap().contains("发起"));

    // 合法配置可以保存，版本号递增
    let before = app.get(&format!("/api/flows/defs/{def_id}"), Some(&admin)).await;
    let version_before = before.body["version"].as_i64().unwrap();

    let ok = app
        .put(
            &format!("/api/flows/defs/{def_id}"),
            json!({
                "code": "default",
                "name": "默认流转",
                "description": "改过说明",
                "config": before.body["config"],
            }),
            Some(&admin),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);

    let after = app.get(&format!("/api/flows/defs/{def_id}"), Some(&admin)).await;
    assert_eq!(after.body["version"].as_i64().unwrap(), version_before + 1);
    assert_eq!(after.body["description"], "改过说明");
}

#[tokio::test]
async fn a_definition_in_use_cannot_be_deleted() {
    let app = TestApp::new();
    let admin = app.admin().await;
    // 默认流程的分派步骤按角色找人，先得有调度员，否则发起就会失败
    add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def_id(&app, &admin).await;
    let created = app
        .post("/api/flows", json!({ "defId": def_id, "title": "占用中" }), Some(&admin))
        .await;
    assert_eq!(created.status, StatusCode::OK, "发起失败: {:?}", created.body);

    let response = app.delete(&format!("/api/flows/defs/{def_id}"), &admin).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(response.body["message"].as_str().unwrap().contains("在用"));
}

#[tokio::test]
async fn starting_without_a_dispatcher_fails_with_a_helpful_message() {
    let app = TestApp::new();
    let admin = app.admin().await;

    // 没有任何用户具备调度员角色
    let def_id = default_def_id(&app, &admin).await;
    let response = app
        .post("/api/flows", json!({ "defId": def_id, "title": "没人可接" }), Some(&admin))
        .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(
        response.body["message"].as_str().unwrap().contains("分派"),
        "应指出是哪一步没人：{}",
        response.body["message"]
    );

    // 失败时不应该留下半截流程
    let list = app.get("/api/flows/instances?scope=all", Some(&admin)).await;
    assert!(list.body.as_array().unwrap().is_empty());
}
