pub mod engine;
pub mod service;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// 结束标记，写在 `next` / `goto` 里表示流程到此结束。
pub const END: &str = "end";

// ---------------- 流程定义 ----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    /// 发起。实例建立时自动通过，不产生任务
    Start,
    /// 分派。办理人选择下一步的承办人
    Dispatch,
    /// 承办。被指派的人做事
    Handle,
    /// 确认。通过则继续，打回则退回指定步骤
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CompleteRule {
    /// 所有承办人都完成才算这一步完成
    #[default]
    All,
    /// 任意一个人完成即可
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AssignMode {
    /// 办理时手动选人
    #[default]
    Manual,
    /// 按角色自动分配
    Role,
}

/// 谁来做这一步。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum Assignee {
    /// 该角色下的所有人
    Role { roles: Vec<String> },
    /// 指定的人
    Users { user_ids: Vec<i64> },
    /// 发起人本人
    #[default]
    Initiator,
    /// 上一步分派时选中的人
    Assigned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepConfig {
    pub key: String,
    pub name: String,
    pub kind: StepKind,
    #[serde(default)]
    pub assignee: Assignee,
    /// 通过后去哪一步；留空表示按顺序进入下一个步骤，最后一个步骤留空即结束
    #[serde(default)]
    pub next: Option<String>,
    /// confirm 被打回时退回到哪一步
    #[serde(default)]
    pub on_reject: Option<String>,
    #[serde(default)]
    pub complete_rule: CompleteRule,
    #[serde(default)]
    pub assign_mode: AssignMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConditionOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Contains,
    Empty,
    NotEmpty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    /// 表单字段或记录字段的标识
    pub field: String,
    pub op: ConditionOp,
    #[serde(default)]
    pub value: Value,
}

/// 简单判断：在某一步通过后，如果条件命中就改去另一条分支。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    /// 在这一步完成之后评估
    pub on_step: String,
    pub when: Condition,
    pub goto: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowConfig {
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
}

impl FlowConfig {
    pub fn parse(raw: &str) -> AppResult<Self> {
        let config: FlowConfig = serde_json::from_str(raw)
            .map_err(|err| AppError::bad_request(format!("流程配置格式不正确：{err}")))?;
        config.validate()?;
        Ok(config)
    }

    pub fn step(&self, key: &str) -> Option<&StepConfig> {
        self.steps.iter().find(|step| step.key == key)
    }

    /// 某个步骤之后默认去哪：先看 next，再看列表顺序。
    pub fn default_next(&self, key: &str) -> Option<String> {
        let index = self.steps.iter().position(|step| step.key == key)?;

        match &self.steps[index].next {
            Some(next) => Some(next.clone()),
            None => self.steps.get(index + 1).map(|step| step.key.clone()),
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.steps.is_empty() {
            return Err(AppError::bad_request("流程至少要有一个步骤"));
        }

        let mut seen = std::collections::HashSet::new();
        for step in &self.steps {
            if step.key.trim().is_empty() {
                return Err(AppError::bad_request("步骤标识不能为空"));
            }
            if step.name.trim().is_empty() {
                return Err(AppError::bad_request(format!("步骤 {} 缺少名称", step.key)));
            }
            if !seen.insert(step.key.as_str()) {
                return Err(AppError::bad_request(format!("步骤标识 {} 重复", step.key)));
            }
        }

        if self.steps[0].kind != StepKind::Start {
            return Err(AppError::bad_request("第一个步骤必须是「发起」"));
        }
        if self.steps.iter().skip(1).any(|step| step.kind == StepKind::Start) {
            return Err(AppError::bad_request("只能有一个「发起」步骤"));
        }

        let keys: std::collections::HashSet<&str> =
            self.steps.iter().map(|step| step.key.as_str()).collect();

        let check_target = |target: &str, what: &str| -> AppResult<()> {
            if target == END || keys.contains(target) {
                Ok(())
            } else {
                Err(AppError::bad_request(format!(
                    "{what}指向了不存在的步骤：{target}"
                )))
            }
        };

        for step in &self.steps {
            if let Some(next) = &step.next {
                check_target(next, &format!("步骤「{}」的下一步", step.name))?;
            }
            if let Some(reject) = &step.on_reject {
                check_target(reject, &format!("步骤「{}」的打回去向", step.name))?;
            }
            if step.kind == StepKind::Confirm && step.on_reject.is_none() {
                return Err(AppError::bad_request(format!(
                    "确认步骤「{}」需要指定打回到哪一步",
                    step.name
                )));
            }
            if step.kind == StepKind::Dispatch && step.assign_mode == AssignMode::Manual {
                // 手动分派必须有人可选，且下一步要接承办
                match self.default_next(&step.key) {
                    Some(next) if keys.contains(next.as_str()) => {}
                    _ => {
                        return Err(AppError::bad_request(format!(
                            "分派步骤「{}」后面需要接一个承办步骤",
                            step.name
                        )));
                    }
                }
            }
        }

        for rule in &self.rules {
            if !keys.contains(rule.on_step.as_str()) {
                return Err(AppError::bad_request(format!(
                    "判断条件挂在了不存在的步骤上：{}",
                    rule.on_step
                )));
            }
            check_target(&rule.goto, "判断条件的目标")?;
            if rule.when.field.trim().is_empty() {
                return Err(AppError::bad_request("判断条件缺少字段"));
            }
        }

        Ok(())
    }
}

/// 截图里那套流程，作为新库的默认模板：
/// 1 发起 → 2 分派 → 3/4/5 承办 → 1 或 2 确认回复，不通过则退回分派。
pub fn default_config() -> FlowConfig {
    serde_json::from_value(default_config_json()).expect("默认流程配置必须是合法的")
}

fn default_config_json() -> Value {
    serde_json::json!({
        "steps": [
            {
                "key": "start",
                "name": "发起",
                "kind": "start",
                "assignee": { "mode": "initiator" }
            },
            {
                "key": "dispatch",
                "name": "分派",
                "kind": "dispatch",
                "assignee": { "mode": "role", "roles": ["dispatcher"] },
                "assignMode": "manual",
                "next": "handle"
            },
            {
                "key": "handle",
                "name": "承办",
                "kind": "handle",
                "assignee": { "mode": "assigned" },
                "completeRule": "all",
                "next": "confirm"
            },
            {
                "key": "confirm",
                "name": "确认回复",
                "kind": "confirm",
                "assignee": { "mode": "role", "roles": ["dispatcher"] },
                "onReject": "dispatch"
            }
        ],
        "rules": []
    })
}

// ---------------- 条件求值 ----------------

/// 在合并后的数据（记录字段 + 表单字段）上判断条件是否成立。
pub fn evaluate(condition: &Condition, data: &serde_json::Map<String, Value>) -> bool {
    let actual = data.get(&condition.field);
    let expected = &condition.value;

    match condition.op {
        ConditionOp::Empty => is_empty(actual),
        ConditionOp::NotEmpty => !is_empty(actual),
        ConditionOp::Eq => compare(actual, expected) == Some(std::cmp::Ordering::Equal),
        ConditionOp::Ne => compare(actual, expected) != Some(std::cmp::Ordering::Equal),
        ConditionOp::Gt => compare(actual, expected) == Some(std::cmp::Ordering::Greater),
        ConditionOp::Lt => compare(actual, expected) == Some(std::cmp::Ordering::Less),
        ConditionOp::Contains => match actual {
            Some(Value::String(text)) => expected
                .as_str()
                .map(|needle| text.contains(needle))
                .unwrap_or(false),
            Some(Value::Array(items)) => items.contains(expected),
            _ => false,
        },
    }
}

fn is_empty(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.trim().is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        _ => false,
    }
}

/// 数字按数值比，其余按文本比；类型对不上就返回 None。
fn compare(actual: Option<&Value>, expected: &Value) -> Option<std::cmp::Ordering> {
    let actual = actual?;

    if let (Some(a), Some(b)) = (actual.as_f64(), expected.as_f64()) {
        return a.partial_cmp(&b);
    }

    match (actual, expected) {
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        // 数字和文本混着比时，统一按文本比较，避免因为类型不一致直接判不成立
        _ => Some(as_text(actual).cmp(&as_text(expected))),
    }
}

fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => if *flag { "是" } else { "否" }.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(key: &str, kind: StepKind) -> StepConfig {
        StepConfig {
            key: key.into(),
            name: key.into(),
            kind,
            assignee: Assignee::Initiator,
            next: None,
            on_reject: None,
            complete_rule: CompleteRule::All,
            assign_mode: AssignMode::Manual,
        }
    }

    fn config_of(steps: Vec<StepConfig>) -> FlowConfig {
        FlowConfig {
            steps,
            rules: vec![],
        }
    }

    #[test]
    fn rejects_empty_or_misordered_steps() {
        assert!(config_of(vec![]).validate().is_err());

        let mut config = config_of(vec![step("s1", StepKind::Handle)]);
        assert!(config.validate().is_err(), "第一步必须是发起");

        config.steps = vec![step("s1", StepKind::Start), step("s2", StepKind::Start)];
        assert!(config.validate().is_err(), "只能有一个发起步骤");
    }

    #[test]
    fn rejects_unknown_targets() {
        let mut config = config_of(vec![step("s1", StepKind::Start), step("s2", StepKind::Handle)]);
        config.steps[0].next = Some("nowhere".into());
        assert!(config.validate().is_err());

        config.steps[0].next = Some(END.into());
        assert!(config.validate().is_ok(), "end 是合法的终点");
    }

    #[test]
    fn confirm_step_requires_reject_target() {
        let mut confirm = step("s2", StepKind::Confirm);
        let config = config_of(vec![step("s1", StepKind::Start), confirm.clone()]);
        assert!(config.validate().is_err());

        confirm.on_reject = Some("s1".into());
        assert!(config_of(vec![step("s1", StepKind::Start), confirm]).validate().is_ok());
    }

    #[test]
    fn dispatch_step_needs_a_following_step() {
        let config = config_of(vec![step("s1", StepKind::Start), step("s2", StepKind::Dispatch)]);
        assert!(config.validate().is_err(), "分派后面没有承办步骤");
    }

    #[test]
    fn default_next_follows_declared_then_order() {
        let mut flow = config_of(vec![
            step("s1", StepKind::Start),
            step("s2", StepKind::Dispatch),
            step("s3", StepKind::Handle),
        ]);
        assert_eq!(flow.default_next("s1").as_deref(), Some("s2"));
        assert_eq!(flow.default_next("s3"), None, "最后一步之后就是结束");

        flow.steps[0].next = Some("s3".into());
        assert_eq!(flow.default_next("s1").as_deref(), Some("s3"));
    }

    #[test]
    fn conditions_compare_numbers_and_text() {
        let mut data = serde_json::Map::new();
        data.insert("qty".into(), json!(10));
        data.insert("level".into(), json!("特急"));

        let eq = |field: &str, op: ConditionOp, value: Value| Condition {
            field: field.into(),
            op,
            value,
        };

        assert!(evaluate(&eq("qty", ConditionOp::Gt, json!(5)), &data));
        assert!(!evaluate(&eq("qty", ConditionOp::Lt, json!(5)), &data));
        assert!(evaluate(&eq("level", ConditionOp::Eq, json!("特急")), &data));
        assert!(evaluate(&eq("level", ConditionOp::Contains, json!("急")), &data));
        assert!(evaluate(&eq("missing", ConditionOp::Empty, json!(null)), &data));
        assert!(evaluate(&eq("qty", ConditionOp::NotEmpty, json!(null)), &data));

        // 数字与文本混比时按文本，不应该因为类型不同就直接不成立
        data.insert("code".into(), json!("10"));
        assert!(evaluate(&eq("code", ConditionOp::Eq, json!(10)), &data));
    }
}
