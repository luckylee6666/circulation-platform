//! 流转状态机。全部是纯函数：输入当前状态，输出要对数据库做的改动。
//!
//! 这样引擎的正确性可以完全用单测覆盖，不必起库。

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::{Assignee, CompleteRule, END, FlowConfig, StepKind, evaluate};

/// 角色成员表。由业务层从数据库装配好传进来，引擎本身不碰数据库。
#[derive(Debug, Default, Clone)]
pub struct Roster {
    pub roles: HashMap<String, Vec<i64>>,
}

impl Roster {
    pub fn members(&self, codes: &[String]) -> Vec<i64> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        for code in codes {
            for id in self.roles.get(code).into_iter().flatten() {
                if seen.insert(*id) {
                    out.push(*id);
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Done,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: i64,
    pub assignee_id: i64,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    /// 承办完成
    Complete,
    /// 确认通过
    Pass,
    /// 确认打回
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub step_key: String,
    pub assignee_id: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// 推进到哪一步。None 表示停在原地等其他人完成
    pub advance_to: Option<String>,
    pub finish: bool,
    pub create_tasks: Vec<NewTask>,
    pub cancel_tasks: Vec<i64>,
}

impl Plan {
    fn moved_to(step_key: String, tasks: Vec<NewTask>) -> Self {
        Self {
            advance_to: Some(step_key),
            finish: false,
            create_tasks: tasks,
            cancel_tasks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowError {
    UnknownStep(String),
    MissingAssignee(String),
    InvalidTransition(String),
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::UnknownStep(key) => write!(f, "流程配置里找不到步骤 {key}"),
            FlowError::MissingAssignee(name) => {
                write!(f, "步骤「{name}」没有可指派的人，请检查角色下是否还有启用的用户")
            }
            FlowError::InvalidTransition(message) => write!(f, "{message}"),
        }
    }
}

/// 发起流程：跳过发起步骤，直接为下一步创建任务。
pub fn start(
    config: &FlowConfig,
    roster: &Roster,
    initiator_id: i64,
    data: &Map<String, Value>,
) -> Result<Plan, FlowError> {
    let first = config
        .steps
        .first()
        .ok_or_else(|| FlowError::UnknownStep("<empty>".into()))?;

    if first.kind != StepKind::Start {
        return Err(FlowError::InvalidTransition(
            "流程的第一个步骤必须是发起".into(),
        ));
    }

    let Some(target) = resolve_next(config, &first.key, data)? else {
        return Ok(Plan {
            finish: true,
            ..Plan::default()
        });
    };

    let tasks = build_tasks(config, roster, &target, initiator_id, &[])?;
    Ok(Plan::moved_to(target, tasks))
}

/// 完成一个任务，返回接下来要做的改动。
#[allow(clippy::too_many_arguments)]
pub fn complete(
    config: &FlowConfig,
    roster: &Roster,
    initiator_id: i64,
    current_step_key: &str,
    tasks: &[TaskRow],
    completed_task_id: i64,
    action: TaskAction,
    assigned_to: &[i64],
    data: &Map<String, Value>,
) -> Result<Plan, FlowError> {
    let step = config
        .step(current_step_key)
        .ok_or_else(|| FlowError::UnknownStep(current_step_key.to_string()))?;

    // 打回：直接退回指定步骤，不管完成规则
    if action == TaskAction::Reject {
        let target = step
            .on_reject
            .clone()
            .ok_or_else(|| FlowError::InvalidTransition("这一步没有配置打回去向".into()))?;

        if target == END {
            return Ok(Plan {
                finish: true,
                ..Plan::default()
            });
        }

        let created = build_tasks(config, roster, &target, initiator_id, assigned_to)?;
        return Ok(Plan::moved_to(target, created));
    }

    // 判断这一步是否已经可以推进：把本次完成的任务算进去后，还有没有人在等
    let mut pending_ids = Vec::new();
    for task in tasks {
        let status = if task.id == completed_task_id {
            TaskStatus::Done
        } else {
            task.status
        };
        if status == TaskStatus::Pending {
            pending_ids.push(task.id);
        }
    }

    let ready = match step.complete_rule {
        CompleteRule::Any => true,
        CompleteRule::All => pending_ids.is_empty(),
    };

    if !ready {
        // 还有别人没做完，什么都不用改
        return Ok(Plan::default());
    }

    // 任一完成时，其余待办要作废，避免流程已经走了还有人能提交
    let cancel_tasks = if step.complete_rule == CompleteRule::Any {
        pending_ids
    } else {
        Vec::new()
    };

    let Some(target) = resolve_next(config, current_step_key, data)? else {
        return Ok(Plan {
            finish: true,
            cancel_tasks,
            ..Plan::default()
        });
    };

    let created = build_tasks(config, roster, &target, initiator_id, assigned_to)?;

    Ok(Plan {
        advance_to: Some(target),
        finish: false,
        create_tasks: created,
        cancel_tasks,
    })
}

/// 按配置里的判断条件找下一个步骤，没有命中就按顺序走。
fn resolve_next(
    config: &FlowConfig,
    step_key: &str,
    data: &Map<String, Value>,
) -> Result<Option<String>, FlowError> {
    for rule in &config.rules {
        if rule.on_step == step_key && evaluate(&rule.when, data) {
            return Ok(if rule.goto == END {
                None
            } else {
                Some(rule.goto.clone())
            });
        }
    }

    Ok(config.default_next(step_key).filter(|key| key != END))
}

/// 算出某个步骤该由哪些人来做。
fn build_tasks(
    config: &FlowConfig,
    roster: &Roster,
    step_key: &str,
    initiator_id: i64,
    assigned: &[i64],
) -> Result<Vec<NewTask>, FlowError> {
    let step = config
        .step(step_key)
        .ok_or_else(|| FlowError::UnknownStep(step_key.to_string()))?;

    let assignees: Vec<i64> = match &step.assignee {
        Assignee::Initiator => vec![initiator_id],
        Assignee::Users { user_ids } => user_ids.clone(),
        Assignee::Role { roles } => roster.members(roles),
        Assignee::Assigned => assigned.to_vec(),
    };

    let mut seen = HashSet::new();
    let unique: Vec<i64> = assignees.into_iter().filter(|id| seen.insert(*id)).collect();

    if unique.is_empty() {
        // 「等上一步指派」为空是操作漏了，和角色下没人不是同一类问题
        return match &step.assignee {
            Assignee::Assigned => Err(FlowError::InvalidTransition(
                "请先选择要指派给谁".to_string(),
            )),
            _ => Err(FlowError::MissingAssignee(step.name.clone())),
        };
    }

    Ok(unique
        .into_iter()
        .map(|assignee_id| NewTask {
            step_key: step_key.to_string(),
            assignee_id,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::flow::{
        AssignMode, Assignee, Condition, ConditionOp, FlowConfig, RoutingRule, StepConfig, StepKind,
    };
    use serde_json::json;

    fn step(key: &str, kind: StepKind, assignee: Assignee) -> StepConfig {
        StepConfig {
            key: key.into(),
            name: key.into(),
            kind,
            assignee,
            next: None,
            on_reject: None,
            complete_rule: CompleteRule::All,
            assign_mode: AssignMode::Manual,
        }
    }

    fn role(codes: &[&str]) -> Assignee {
        Assignee::Role {
            roles: codes.iter().map(|code| (*code).to_string()).collect(),
        }
    }

    fn roster() -> Roster {
        let mut roles = HashMap::new();
        roles.insert("dispatcher".to_string(), vec![20]);
        roles.insert("handler".to_string(), vec![31, 32, 33]);
        Roster { roles }
    }

    /// 截图里那套流程：1 发起 → 2 分派 → 3/4/5 承办 → 1 或 2 确认
    fn default_flow() -> FlowConfig {
        let mut dispatch = step("s2", StepKind::Dispatch, role(&["dispatcher"]));
        dispatch.next = Some("s3".into());

        let handle = step("s3", StepKind::Handle, Assignee::Assigned);

        let mut confirm = step("s4", StepKind::Confirm, role(&["dispatcher"]));
        confirm.on_reject = Some("s2".into());

        FlowConfig {
            steps: vec![
                step("s1", StepKind::Start, Assignee::Initiator),
                dispatch,
                handle,
                confirm,
            ],
            rules: vec![],
        }
    }

    #[test]
    fn start_skips_the_start_step_and_assigns_the_dispatcher() {
        let plan = start(&default_flow(), &roster(), 1, &Map::new()).unwrap();

        assert_eq!(plan.advance_to.as_deref(), Some("s2"));
        assert_eq!(
            plan.create_tasks,
            vec![NewTask {
                step_key: "s2".into(),
                assignee_id: 20
            }]
        );
        assert!(!plan.finish);
    }

    #[test]
    fn role_without_members_is_reported() {
        let mut config = default_flow();
        config.steps[1].assignee = role(&["nobody"]);
        let error = start(&config, &roster(), 1, &Map::new()).unwrap_err();
        assert!(matches!(error, FlowError::MissingAssignee(_)));
    }

    #[test]
    fn dispatch_hands_work_to_the_chosen_people() {
        let config = default_flow();
        let tasks = vec![TaskRow {
            id: 100,
            assignee_id: 20,
            status: TaskStatus::Pending,
        }];

        let plan = complete(
            &config,
            &roster(),
            1,
            "s2",
            &tasks,
            100,
            TaskAction::Complete,
            &[31, 32],
            &Map::new(),
        )
        .unwrap();

        assert_eq!(plan.advance_to.as_deref(), Some("s3"));
        assert_eq!(
            plan.create_tasks,
            vec![
                NewTask {
                    step_key: "s3".into(),
                    assignee_id: 31
                },
                NewTask {
                    step_key: "s3".into(),
                    assignee_id: 32
                },
            ]
        );
    }

    #[test]
    fn all_rule_waits_for_every_handler() {
        let config = default_flow();
        let tasks = vec![
            TaskRow { id: 1, assignee_id: 31, status: TaskStatus::Pending },
            TaskRow { id: 2, assignee_id: 32, status: TaskStatus::Pending },
            TaskRow { id: 3, assignee_id: 33, status: TaskStatus::Pending },
        ];

        // 第一个人做完，还不动
        let plan = complete(&config, &roster(), 1, "s3", &tasks, 1, TaskAction::Complete, &[], &Map::new()).unwrap();
        assert_eq!(plan, Plan::default(), "还有人没做完时不应推进");

        // 第二个人做完，还差一个
        let tasks = vec![
            TaskRow { id: 1, assignee_id: 31, status: TaskStatus::Done },
            TaskRow { id: 2, assignee_id: 32, status: TaskStatus::Pending },
            TaskRow { id: 3, assignee_id: 33, status: TaskStatus::Pending },
        ];
        let plan = complete(&config, &roster(), 1, "s3", &tasks, 2, TaskAction::Complete, &[], &Map::new()).unwrap();
        assert_eq!(plan, Plan::default());

        // 最后一个人做完，进入确认
        let tasks = vec![
            TaskRow { id: 1, assignee_id: 31, status: TaskStatus::Done },
            TaskRow { id: 2, assignee_id: 32, status: TaskStatus::Done },
            TaskRow { id: 3, assignee_id: 33, status: TaskStatus::Pending },
        ];
        let plan = complete(&config, &roster(), 1, "s3", &tasks, 3, TaskAction::Complete, &[], &Map::new()).unwrap();
        assert_eq!(plan.advance_to.as_deref(), Some("s4"));
    }

    #[test]
    fn any_rule_advances_immediately_and_cancels_the_rest() {
        let mut config = default_flow();
        config.steps[2].complete_rule = CompleteRule::Any;

        let tasks = vec![
            TaskRow { id: 1, assignee_id: 31, status: TaskStatus::Pending },
            TaskRow { id: 2, assignee_id: 32, status: TaskStatus::Pending },
            TaskRow { id: 3, assignee_id: 33, status: TaskStatus::Pending },
        ];

        let plan = complete(&config, &roster(), 1, "s3", &tasks, 1, TaskAction::Complete, &[], &Map::new()).unwrap();

        assert_eq!(plan.advance_to.as_deref(), Some("s4"));
        assert_eq!(plan.cancel_tasks, vec![2, 3]);
    }

    #[test]
    fn confirm_reject_sends_the_flow_back() {
        let config = default_flow();
        let tasks = vec![TaskRow { id: 9, assignee_id: 20, status: TaskStatus::Pending }];

        let plan = complete(&config, &roster(), 1, "s4", &tasks, 9, TaskAction::Reject, &[31], &Map::new()).unwrap();

        assert_eq!(plan.advance_to.as_deref(), Some("s2"), "打回退到分派步骤");
        assert_eq!(plan.create_tasks[0].assignee_id, 20);
    }

    #[test]
    fn last_step_finishes_the_flow() {
        let mut confirm = step("s2", StepKind::Confirm, role(&["dispatcher"]));
        confirm.on_reject = Some("s1".into());
        let config = FlowConfig {
            steps: vec![step("s1", StepKind::Start, Assignee::Initiator), confirm],
            rules: vec![],
        };

        let tasks = vec![TaskRow { id: 1, assignee_id: 20, status: TaskStatus::Pending }];
        let plan = complete(&config, &roster(), 1, "s2", &tasks, 1, TaskAction::Pass, &[], &Map::new()).unwrap();

        assert!(plan.finish);
        assert!(plan.advance_to.is_none());
    }

    #[test]
    fn routing_rule_overrides_the_default_next_step() {
        let mut config = default_flow();
        // 数量大于 100 时跳过承办，直接进确认
        config.rules.push(RoutingRule {
            on_step: "s2".into(),
            when: Condition {
                field: "qty".into(),
                op: ConditionOp::Gt,
                value: json!(100),
            },
            goto: "s4".into(),
        });

        let tasks = vec![TaskRow { id: 100, assignee_id: 20, status: TaskStatus::Pending }];

        let mut data = Map::new();
        data.insert("qty".into(), json!(500));
        let plan = complete(&config, &roster(), 1, "s2", &tasks, 100, TaskAction::Complete, &[31], &data).unwrap();
        assert_eq!(plan.advance_to.as_deref(), Some("s4"));

        // 条件不成立时按默认路径走
        data.insert("qty".into(), json!(5));
        let plan = complete(&config, &roster(), 1, "s2", &tasks, 100, TaskAction::Complete, &[31], &data).unwrap();
        assert_eq!(plan.advance_to.as_deref(), Some("s3"));
    }

    #[test]
    fn rule_can_end_the_flow_early() {
        let mut config = default_flow();
        config.rules.push(RoutingRule {
            on_step: "s2".into(),
            when: Condition {
                field: "qty".into(),
                op: ConditionOp::Eq,
                value: json!(0),
            },
            goto: END.into(),
        });

        let tasks = vec![TaskRow { id: 100, assignee_id: 20, status: TaskStatus::Pending }];
        let mut data = Map::new();
        data.insert("qty".into(), json!(0));

        let plan = complete(&config, &roster(), 1, "s2", &tasks, 100, TaskAction::Complete, &[], &data).unwrap();
        assert!(plan.finish);
    }

    #[test]
    fn duplicate_assignees_are_merged() {
        let mut config = default_flow();
        config.steps[1].assignee = Assignee::Users {
            user_ids: vec![7, 7, 8],
        };
        let plan = start(&config, &roster(), 1, &Map::new()).unwrap();

        let ids: Vec<i64> = plan.create_tasks.iter().map(|task| task.assignee_id).collect();
        assert_eq!(ids, vec![7, 8]);
    }
}
