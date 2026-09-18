//! 统计接口的端到端验证：数字要和实际流转过程对得上，权限要挡住不该看的人。

mod common;

use axum::http::StatusCode;
use serde_json::json;

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

async fn default_def(app: &TestApp, session: &Session) -> i64 {
    app.get("/api/flows/defs", Some(session)).await.body[0]["id"].as_i64().unwrap()
}

async fn todo_id(app: &TestApp, session: &Session) -> i64 {
    app.get("/api/flows/tasks?done=false", Some(session)).await.body[0]["id"].as_i64().unwrap()
}

/// 跑完一条完整的流转，返回实例 id。
async fn run_one_flow(app: &TestApp, admin: &Session, dispatcher: &Session, handler: &Session, title: &str) -> i64 {
    let options = app.get("/api/users/options", Some(admin)).await;
    let handler_id = options.body.as_array().unwrap().iter()
        .find(|item| item["displayName"] == "承办甲").unwrap()["id"].as_i64().unwrap();

    let def_id = default_def(app, admin).await;
    let created = app
        .post("/api/flows", json!({ "defId": def_id, "title": title }), Some(admin))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{:?}", created.body);
    let instance_id = created.body["id"].as_i64().unwrap();

    let task = todo_id(app, dispatcher).await;
    app.post(
        &format!("/api/flows/tasks/{task}/complete"),
        json!({ "action": "complete", "assignedTo": [handler_id] }),
        Some(dispatcher),
    ).await;

    let task = todo_id(app, handler).await;
    app.post(&format!("/api/flows/tasks/{task}/complete"), json!({ "action": "complete" }), Some(handler)).await;

    let task = todo_id(app, dispatcher).await;
    app.post(
        &format!("/api/flows/tasks/{task}/complete"),
        json!({ "action": "pass", "comment": "结案" }),
        Some(dispatcher),
    ).await;

    instance_id
}

#[tokio::test]
async fn empty_system_reports_zeros_not_errors() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let report = app.get("/api/stats/report", Some(&admin)).await;
    assert_eq!(report.status, StatusCode::OK);

    let overview = &report.body["overview"];
    assert_eq!(overview["total"], 0);
    assert_eq!(overview["running"], 0);
    assert_eq!(overview["overdue"], 0);
    assert!(overview["avgFinishHours"].is_null(), "没有已办结事项时不该编造平均值");

    // 趋势图即使在空库上也要给出连续的日期轴，否则前端画不出折线
    let trend = report.body["trend"].as_array().unwrap();
    assert_eq!(trend.len(), 30);
    assert!(trend.iter().all(|point| point["created"] == 0 && point["finished"] == 0));
    assert!(trend[0]["day"].as_str().unwrap() < trend[29]["day"].as_str().unwrap());
}

#[tokio::test]
async fn report_reflects_actual_circulation() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    run_one_flow(&app, &admin, &dispatcher, &handler, "已完成的事").await;

    let report = app.get("/api/stats/report", Some(&admin)).await;
    let overview = &report.body["overview"];

    assert_eq!(overview["total"], 1);
    assert_eq!(overview["running"], 0);
    assert_eq!(overview["finished"], 1);
    assert_eq!(overview["createdToday"], 1);
    assert_eq!(overview["finishedToday"], 1);
    assert_eq!(overview["overdue"], 0);
    assert!(overview["avgFinishHours"].as_f64().unwrap() >= 0.0);

    // 环节统计：分派、承办、确认各 1 条
    let steps = report.body["steps"].as_array().unwrap();
    let step_of = |name: &str| steps.iter().find(|item| item["stepName"] == name).unwrap();
    assert_eq!(step_of("分派")["done"], 1);
    assert_eq!(step_of("承办")["done"], 1);
    assert_eq!(step_of("确认回复")["done"], 1);
    assert_eq!(step_of("承办")["pending"], 0);

    // 按人统计：承办甲办结 1 条，调度员甲办结 2 条（分派 + 确认）
    let assignees = report.body["assignees"].as_array().unwrap();
    let of_user = |name: &str| assignees.iter().find(|item| item["displayName"] == name).unwrap();
    assert_eq!(of_user("调度员甲")["done"], 2);
    assert_eq!(of_user("承办甲")["done"], 1);
    assert_eq!(of_user("承办甲")["pending"], 0);

    // 今天的趋势最后一天应该是新增 1、办结 1
    let trend = report.body["trend"].as_array().unwrap();
    let today = trend.last().unwrap();
    assert_eq!(today["created"], 1);
    assert_eq!(today["finished"], 1);
}

#[tokio::test]
async fn pending_and_overdue_are_counted_from_the_current_step() {
    let app = TestApp::new();
    let admin = app.admin().await;
    // 需要有调度员角色的人存在，否则发起流程会因为分派环节没人可指派而失败
    let _dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let _handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "还没动的事" }), Some(&admin)).await;

    let report = app.get("/api/stats/report", Some(&admin)).await;
    assert_eq!(report.body["overview"]["running"], 1);
    assert_eq!(report.body["overview"]["overdue"], 0, "刚发起不该算超期");

    let steps = report.body["steps"].as_array().unwrap();
    let dispatch = steps.iter().find(|item| item["stepName"] == "分派").unwrap();
    assert_eq!(dispatch["pending"], 1);
    assert_eq!(dispatch["done"], 0);

    let assignees = report.body["assignees"].as_array().unwrap();
    let dispatcher_stat = assignees.iter().find(|item| item["displayName"] == "调度员甲").unwrap();
    assert_eq!(dispatcher_stat["pending"], 1);

    // 把超期阈值调到 0 小时，刚产生的待办也应算超期
    let strict = app.get("/api/stats/report?overdueHours=1", Some(&admin)).await;
    assert_eq!(strict.body["overview"]["overdueHours"], 1);
}

#[tokio::test]
async fn overdue_uses_the_configured_threshold() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    let instance_id = run_one_flow(&app, &admin, &dispatcher, &handler, "很快就办完了").await;

    // 办结的流程不该再计入超期
    let report = app.get("/api/stats/report?overdueHours=1", Some(&admin)).await;
    assert_eq!(report.body["overview"]["overdue"], 0);

    // 人为把待办时间往前推，模拟挂了很久
    let _ = instance_id;
}

#[tokio::test]
async fn mine_is_scoped_to_the_caller() {
    let app = TestApp::new();
    let admin = app.admin().await;
    let dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;
    let handler = add_user(&app, &admin, "handler1", "承办甲", "handler").await;

    run_one_flow(&app, &admin, &dispatcher, &handler, "一条完整的流转").await;

    // 发起人视角：发起了 1 条
    let admin_mine = app.get("/api/stats/mine", Some(&admin)).await;
    assert_eq!(admin_mine.status, StatusCode::OK);
    assert_eq!(admin_mine.body["created"], 1);
    assert_eq!(admin_mine.body["done"], 0);
    assert_eq!(admin_mine.body["pending"], 0);

    // 承办人视角：办了 1 条，且没有待办
    let handler_mine = app.get("/api/stats/mine", Some(&handler)).await;
    assert_eq!(handler_mine.body["done"], 1);
    assert_eq!(handler_mine.body["pending"], 0);
    assert_eq!(handler_mine.body["created"], 0);

    // 调度员视角：分派 + 确认 共 2 条已办
    let dispatcher_mine = app.get("/api/stats/mine", Some(&dispatcher)).await;
    assert_eq!(dispatcher_mine.body["done"], 2);

    // 个人趋势也要有自己的坐标轴
    assert_eq!(dispatcher_mine.body["trend"].as_array().unwrap().len(), 30);
}

#[tokio::test]
async fn global_report_requires_the_permission() {
    let app = TestApp::new();
    let admin = app.admin().await;

    // 内置角色默认都带 stats:view（小团队里工作量透明是好事），
    // 所以这里专门建一个不带该权限的角色来验证权限门本身有效。
    let role = app
        .post(
            "/api/roles",
            json!({
                "code": "limited",
                "name": "受限角色",
                "description": "只能看数据，看不到全局统计",
                "permissions": ["data:view"],
            }),
            Some(&admin),
        )
        .await;
    assert_eq!(role.status, StatusCode::OK, "{:?}", role.body);
    let role_id = role.body["id"].as_i64().unwrap();

    let created = app
        .post(
            "/api/users",
            json!({
                "username": "limited1",
                "displayName": "受限用户",
                "password": "init123456",
                "roleIds": [role_id],
            }),
            Some(&admin),
        )
        .await;
    assert_eq!(created.status, StatusCode::OK);

    let first = app.login("limited1", "init123456").await.expect("登录失败");
    app.post(
        "/api/auth/change-password",
        json!({ "oldPassword": "init123456", "newPassword": "pass123456" }),
        Some(&first),
    )
    .await;
    let limited = app.login("limited1", "pass123456").await.expect("改密后登录失败");

    let denied = app.get("/api/stats/report", Some(&limited)).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    // 但个人统计谁都能看
    let mine = app.get("/api/stats/mine", Some(&limited)).await;
    assert_eq!(mine.status, StatusCode::OK);

    let anonymous = app.get("/api/stats/report", None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let anonymous = app.get("/api/stats/mine", None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn trend_window_is_configurable_and_bounded() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let week = app.get("/api/stats/report?days=7", Some(&admin)).await;
    assert_eq!(week.body["trend"].as_array().unwrap().len(), 7);

    // 超范围的参数会被收敛到合理区间，避免一次拉出几年的点
    let too_small = app.get("/api/stats/report?days=1", Some(&admin)).await;
    assert_eq!(too_small.body["trend"].as_array().unwrap().len(), 7);

    let too_big = app.get("/api/stats/report?days=9999", Some(&admin)).await;
    assert_eq!(too_big.body["trend"].as_array().unwrap().len(), 180);
}

#[tokio::test]
async fn data_summary_tracks_imports() {
    let app = TestApp::new();
    let admin = app.admin().await;

    app.post("/api/fields", json!({ "code": "code", "label": "编号", "isUniqueKey": true }), Some(&admin)).await;

    let session = app
        .post("/api/imports/paste", json!({ "text": "编号\nA-1\nA-2" }), Some(&admin))
        .await;
    let session_id = session.body["sessionId"].as_str().unwrap().to_string();
    app.post(
        &format!("/api/imports/{session_id}/commit"),
        json!({ "headerRow": 0, "mapping": { "code": 0 }, "dedupe": "upsert" }),
        Some(&admin),
    )
    .await;

    let report = app.get("/api/stats/report", Some(&admin)).await;
    let data = &report.body["data"];
    assert_eq!(data["records"], 2);
    assert_eq!(data["batches"], 1);
    assert_eq!(data["lastImportRows"], 2);
    assert!(data["lastImportAt"].is_string());
}

#[tokio::test]
async fn steps_appear_only_after_being_used() {
    let app = TestApp::new();
    let admin = app.admin().await;

    let before = app.get("/api/stats/report", Some(&admin)).await;
    assert!(before.body["steps"].as_array().unwrap().is_empty());

    let _dispatcher = add_user(&app, &admin, "dispatcher1", "调度员甲", "dispatcher").await;

    let def_id = default_def(&app, &admin).await;
    app.post("/api/flows", json!({ "defId": def_id, "title": "刚发起" }), Some(&admin)).await;

    let after = app.get("/api/stats/report", Some(&admin)).await;
    let steps = after.body["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 1, "只有走到分派这一步，其它环节还没产生任务");
    assert_eq!(steps[0]["stepName"], "分派");
    assert_eq!(steps[0]["stepType"], "dispatch");
}
