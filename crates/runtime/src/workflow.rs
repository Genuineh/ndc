//! Workflow Engine - 状态机引擎
//!
//! 职责：
//! - 管理 Task 状态流转
//! - 执行状态转换规则
//! - 触发自动化转换
//! - 处理阻塞与恢复

use ndc_core::{Task, TaskState, WorkRecord, WorkEvent, Executor, WorkResult};
use std::collections::HashMap;
use std::hash::Hash;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 工作流错误
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("不允许的状态转换: {from:?} -> {to:?}")]
    InvalidTransition { from: TaskState, to: TaskState },

    #[error("转换条件未满足: {0}")]
    ConditionNotMet(String),

    #[error("Task 不存在: {0}")]
    TaskNotFound(String),

    #[error("自动化转换失败: {0}")]
    AutoTransitionFailed(String),
}

/// 工作流事件
#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    /// 任务创建
    Created { task_id: String },

    /// 状态转换
    Transitioned { task_id: String, from: TaskState, to: TaskState },

    /// 阻塞
    Blocked { task_id: String, reason: String },

    /// 解除阻塞
    Unblocked { task_id: String },

    /// 完成
    Completed { task_id: String },

    /// 失败
    Failed { task_id: String, reason: String },

    /// 自动化转换
    AutoTransition { task_id: String, from: TaskState, to: TaskState },
}

/// 转换条件
#[derive(Debug, Clone)]
pub struct TransitionCondition {
    /// 条件类型
    pub condition_type: ConditionType,

    /// 条件描述
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ConditionType {
    /// 所有前置任务完成
    DependenciesComplete,

    /// 产物存在
    ArtifactsPresent,

    /// 所有测试通过
    AllTestsPassed,

    /// 人类批准
    HumanApproved,

    /// 自定义条件
    Custom(String),
}

/// 转换规则
#[derive(Debug, Clone)]
pub struct TransitionRule {
    /// 源状态
    pub from: TaskState,

    /// 目标状态
    pub to: TaskState,

    /// 所需条件
    pub conditions: Vec<TransitionCondition>,

    /// 允许执行的角色
    pub allowed_roles: Vec<ndc_core::AgentRole>,

    /// 是否自动化转换
    pub auto_transition: bool,

    /// 转换后触发的动作
    pub post_actions: Vec<PostAction>,
}

/// 转换后动作
#[derive(Debug, Clone)]
pub enum PostAction {
    /// 捕获快照
    CaptureSnapshot,

    /// 发送通知
    Notify { channel: String, message: String },

    /// 运行质量检查
    RunQualityCheck { check_type: String },

    /// 触发自动化步骤
    TriggerAutomation { step_name: String },
}

/// 工作流引擎
#[derive(Debug, Default)]
pub struct WorkflowEngine {
    /// 转换规则
    rules: Vec<TransitionRule>,

    /// 状态监听器
    listeners: Vec<Arc<dyn WorkflowListener>>,
}

impl WorkflowEngine {
    /// 创建新的工作流引擎
    pub fn new() -> Self {
        let mut engine = Self::default();
        engine.register_default_rules();
        engine
    }

    /// 注册默认规则
    fn register_default_rules(&mut self) {
        self.rules = vec![
            // Pending -> Preparing: 自动转换，前提条件满足
            TransitionRule {
                from: TaskState::Pending,
                to: TaskState::Preparing,
                conditions: vec![],
                allowed_roles: vec![
                    ndc_core::AgentRole::Planner,
                    ndc_core::AgentRole::Implementer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: true,
                post_actions: vec![PostAction::CaptureSnapshot],
            },

            // Preparing -> InProgress: 前置条件满足
            TransitionRule {
                from: TaskState::Preparing,
                to: TaskState::InProgress,
                conditions: vec![
                    TransitionCondition {
                        condition_type: ConditionType::ArtifactsPresent,
                        description: "上下文已读取".to_string(),
                    },
                ],
                allowed_roles: vec![
                    ndc_core::AgentRole::Implementer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: true,
                post_actions: vec![],
            },

            // InProgress -> AwaitingVerification: 手动或自动
            TransitionRule {
                from: TaskState::InProgress,
                to: TaskState::AwaitingVerification,
                conditions: vec![],
                allowed_roles: vec![
                    ndc_core::AgentRole::Implementer,
                    ndc_core::AgentRole::Reviewer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: true,
                post_actions: vec![PostAction::RunQualityCheck {
                    check_type: "default".to_string(),
                }],
            },

            // AwaitingVerification -> Completed: 测试通过
            TransitionRule {
                from: TaskState::AwaitingVerification,
                to: TaskState::Completed,
                conditions: vec![
                    TransitionCondition {
                        condition_type: ConditionType::AllTestsPassed,
                        description: "所有测试通过".to_string(),
                    },
                ],
                allowed_roles: vec![
                    ndc_core::AgentRole::Reviewer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: true,
                post_actions: vec![],
            },

            // AwaitingVerification -> Failed: 测试失败
            TransitionRule {
                from: TaskState::AwaitingVerification,
                to: TaskState::Failed,
                conditions: vec![],
                allowed_roles: vec![
                    ndc_core::AgentRole::Reviewer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: false,
                post_actions: vec![],
            },

            // InProgress -> Blocked: 需要人工介入
            TransitionRule {
                from: TaskState::InProgress,
                to: TaskState::Blocked,
                conditions: vec![],
                allowed_roles: vec![
                    ndc_core::AgentRole::Implementer,
                    ndc_core::AgentRole::Reviewer,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: false,
                post_actions: vec![PostAction::Notify {
                    channel: "human".to_string(),
                    message: "Task 需要人工介入".to_string(),
                }],
            },

            // Blocked -> InProgress: 人工批准
            TransitionRule {
                from: TaskState::Blocked,
                to: TaskState::InProgress,
                conditions: vec![
                    TransitionCondition {
                        condition_type: ConditionType::HumanApproved,
                        description: "人类已批准".to_string(),
                    },
                ],
                allowed_roles: vec![
                    ndc_core::AgentRole::Human,
                    ndc_core::AgentRole::Historian,
                ],
                auto_transition: false,
                post_actions: vec![],
            },
        ];
    }

    /// 注册监听器
    pub fn register_listener(&mut self, listener: Arc<dyn WorkflowListener>) {
        self.listeners.push(listener);
    }

    /// 获取允许的转换
    pub fn get_allowed_transitions(&self, task: &Task) -> Vec<TaskState> {
        self.rules
            .iter()
            .filter(|rule| rule.from == task.state)
            .filter(|rule| {
                // 检查条件是否满足
                self.check_conditions(&rule.conditions, task)
            })
            .map(|rule| rule.to.clone())
            .collect()
    }

    /// 请求状态转换
    pub async fn request_transition(
        &self,
        task: &mut Task,
        to: TaskState,
        executor: Executor,
    ) -> Result<(), WorkflowError> {
        let from = task.state.clone();

        // 查找转换规则
        let rule = self.rules.iter()
            .find(|r| r.from == from && r.to == to)
            .ok_or_else(|| WorkflowError::InvalidTransition {
                from: from.clone(),
                to: to.clone(),
            })?;

        // 检查条件
        if !self.check_conditions(&rule.conditions, task) {
            return Err(WorkflowError::ConditionNotMet(
                "转换条件未满足".to_string(),
            ));
        }

        // 执行状态转换
        task.request_transition(to)
            .map_err(|e| WorkflowError::InvalidTransition {
                from: from.clone(),
                to: to.clone(),
            })?;

        // 记录工作记录
        let record = WorkRecord {
            id: ulid::Ulid::new(),
            timestamp: ndc_core::Timestamp::now(),
            event: WorkEvent::Transitioned {
                from: from.clone(),
                to: to.clone(),
            },
            executor: Executor::System,
            result: WorkResult::Success,
        };
        task.metadata.work_records.push(record);

        // 触发后置动作
        self.execute_post_actions(task, &rule.post_actions).await;

        // 通知监听器
        for listener in &self.listeners {
            listener.on_event(&WorkflowEvent::Transitioned {
                task_id: task.id.to_string(),
                from,
                to,
            });
        }

        // 检查是否可以自动转换
        if rule.auto_transition {
            self.check_auto_transition(task, executor).await;
        }

        Ok(())
    }

    /// 检查条件是否满足
    fn check_conditions(&self, conditions: &[TransitionCondition], task: &Task) -> bool {
        for condition in conditions {
            match &condition.condition_type {
                ConditionType::DependenciesComplete => {
                    // TODO: 检查前置任务
                }
                ConditionType::ArtifactsPresent => {
                    // 检查产物是否存在
                    if task.metadata.work_records.is_empty() {
                        return false;
                    }
                }
                ConditionType::AllTestsPassed => {
                    // 检查测试结果
                    // TODO: 实现
                }
                ConditionType::HumanApproved => {
                    // 检查是否有人的批准记录
                    let has_human_approval = task.metadata.work_records.iter().any(|r| {
                        matches!(r.event, WorkEvent::Unblocked)
                    });
                    if !has_human_approval && task.state == TaskState::Blocked {
                        return false;
                    }
                }
                ConditionType::Custom(_) => {
                    // TODO: 自定义条件
                }
            }
        }
        true
    }

    /// 执行后置动作
    async fn execute_post_actions(&self, task: &mut Task, actions: &[PostAction]) {
        for action in actions {
            match action {
                PostAction::CaptureSnapshot => {
                    // TODO: 捕获快照
                    debug!("Capturing snapshot for task {}", task.id);
                }
                PostAction::Notify { channel, message } => {
                    debug!("Notifying {}: {}", channel, message);
                }
                PostAction::RunQualityCheck { check_type } => {
                    debug!("Running quality check: {}", check_type);
                }
                PostAction::TriggerAutomation { step_name } => {
                    debug!("Triggering automation: {}", step_name);
                }
            }
        }
    }

    /// 检查自动转换
    async fn check_auto_transition(&self, task: &mut Task, _executor: Executor) {
        let allowed = self.get_allowed_transitions(task);
        if let Some(next_state) = allowed.first() {
            if task.state != *next_state {
                info!("Auto-transitioning task {} from {:?} to {:?}",
                    task.id, task.state, next_state);

                // TODO: 实现自动转换
            }
        }
    }
}

/// 工作流监听器 Trait
#[async_trait::async_trait]
pub trait WorkflowListener: Send + Sync {
    async fn on_event(&self, event: &WorkflowEvent);
}

/// 内存监听器（用于测试）
#[derive(Debug, Default)]
pub struct MemoryWorkflowListener {
    events: std::sync::Mutex<Vec<WorkflowEvent>>,
}

impl MemoryWorkflowListener {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_events(&self) -> Vec<WorkflowEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl WorkflowListener for MemoryWorkflowListener {
    async fn on_event(&self, event: &WorkflowEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}
