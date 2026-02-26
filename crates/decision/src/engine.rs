//! Decision Engine - 决策与约束引擎
//!
//! 职责：
//! - 评估 Intent 并返回 Verdict
//! - 执行约束校验
//! - 权限等级判定
//!
//! 设计原则：
//! - 同步阻塞：没有 Verdict，任何动作不能 commit
//! - 插件不能绕过核心层

use async_trait::async_trait;
use ndc_core::{
    Action, AgentRole, Condition, ConditionType, ErrorCode, HumanContext, Intent, PrivilegeLevel,
    Verdict,
};
use std::collections::HashMap;
use std::sync::Arc;

/// 决策引擎 Trait
#[async_trait]
pub trait DecisionEngine: Send + Sync {
    /// 评估 Intent
    async fn evaluate(&self, intent: Intent) -> Verdict;

    /// 批量评估
    async fn evaluate_batch(&self, intents: Vec<Intent>) -> Vec<Verdict>;

    /// 注册校验器
    fn register_validator(&mut self, validator: Arc<dyn Validator>);

    /// 获取策略状态
    fn policy_state(&self) -> PolicyState;
}

/// 校验器 Trait
#[async_trait]
pub trait Validator: Send + Sync {
    /// 校验 Intent
    async fn validate(&self, intent: &Intent, policy: &PolicyState) -> ValidationResult;

    /// 校验器名称
    fn name(&self) -> &str;

    /// 校验器优先级（数字越小优先级越高）
    fn priority(&self) -> u32;
}

/// 校验结果
#[derive(Debug)]
pub enum ValidationResult {
    Allow,
    Deny(String),
    RequireHuman(String, HumanContext),
    Modify(Action, String, Vec<String>),
    Defer(Vec<ndc_core::InformationRequirement>, Option<u64>),
}

/// 策略状态
#[derive(Debug, Clone, Default)]
pub struct PolicyState {
    /// 是否启用严格模式
    pub strict_mode: bool,

    /// 是否允许危险操作
    pub allow_dangerous: bool,

    /// 最大文件修改数
    pub max_file_modifications: u32,

    /// 是否需要人类确认高风险操作
    pub require_human_for_high_risk: bool,

    /// 活跃规则列表
    pub active_rules: Vec<String>,

    /// 人类介入次数
    pub human_interventions: u32,

    /// 被拒绝的 Intent 数量
    pub denied_intents: u32,
}

/// 决策引擎实现
pub struct BasicDecisionEngine {
    /// 校验器列表（按优先级排序）
    validators: Vec<Arc<dyn Validator + Send + Sync>>,

    /// 策略状态
    policy_state: PolicyState,

    /// 角色权限映射
    role_privileges: HashMap<AgentRole, PrivilegeLevel>,
}

impl BasicDecisionEngine {
    /// 创建新的决策引擎
    pub fn new() -> Self {
        let mut engine = Self {
            validators: Vec::new(),
            policy_state: PolicyState::default(),
            role_privileges: HashMap::new(),
        };

        // 初始化默认角色权限
        engine.init_default_privileges();

        engine
    }

    /// 初始化默认角色权限
    fn init_default_privileges(&mut self) {
        self.role_privileges
            .insert(AgentRole::Planner, PrivilegeLevel::Normal);
        self.role_privileges
            .insert(AgentRole::Implementer, PrivilegeLevel::Elevated);
        self.role_privileges
            .insert(AgentRole::Reviewer, PrivilegeLevel::Normal);
        self.role_privileges
            .insert(AgentRole::Tester, PrivilegeLevel::Normal);
        self.role_privileges
            .insert(AgentRole::Historian, PrivilegeLevel::Normal);
        self.role_privileges
            .insert(AgentRole::Admin, PrivilegeLevel::Critical);
    }
}

impl Default for BasicDecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionEngine for BasicDecisionEngine {
    async fn evaluate(&self, intent: Intent) -> Verdict {
        // 1. 计算所需权限等级
        let required_privilege = self.calculate_required_privilege(&intent);

        // 2. 获取角色默认权限
        let granted_privilege = self
            .role_privileges
            .get(&intent.agent_role)
            .cloned()
            .unwrap_or(PrivilegeLevel::Normal);

        // 3. 按优先级运行校验器
        for validator in &self.validators {
            let result = validator.validate(&intent, &self.policy_state).await;

            match result {
                ValidationResult::Allow => continue,
                ValidationResult::Deny(reason) => {
                    return Verdict::Deny {
                        action: intent.proposed_action,
                        reason,
                        error_code: ErrorCode::InvalidAction,
                    };
                }
                ValidationResult::RequireHuman(question, context) => {
                    return Verdict::RequireHuman {
                        action: intent.proposed_action,
                        question,
                        context,
                        timeout: Some(300),
                    };
                }
                ValidationResult::Modify(modified_action, reason, warnings) => {
                    return Verdict::Modify {
                        original_action: intent.proposed_action,
                        modified_action,
                        reason,
                        warnings,
                    };
                }
                ValidationResult::Defer(info, retry_after) => {
                    return Verdict::Defer {
                        action: intent.proposed_action,
                        required_info: info,
                        retry_after,
                    };
                }
            }
        }

        // 4. 权限检查
        if required_privilege > granted_privilege {
            return Verdict::Deny {
                action: intent.proposed_action,
                reason: format!(
                    "Insufficient privilege: required {:?}, granted {:?}",
                    required_privilege, granted_privilege
                ),
                error_code: ErrorCode::InsufficientPrivilege {
                    required: required_privilege,
                    granted: granted_privilege,
                },
            };
        }

        // 5. 构建附加条件
        let conditions = self.build_conditions(&intent);

        // 6. 返回 Allow Verdict
        Verdict::Allow {
            action: intent.proposed_action,
            privilege: granted_privilege,
            conditions,
        }
    }

    async fn evaluate_batch(&self, intents: Vec<Intent>) -> Vec<Verdict> {
        let mut results = Vec::with_capacity(intents.len());
        for intent in intents {
            results.push(self.evaluate(intent).await);
        }
        results
    }

    fn register_validator(&mut self, validator: Arc<dyn Validator>) {
        self.validators.push(validator);
        self.validators.sort_by_key(|v| v.priority());
    }

    fn policy_state(&self) -> PolicyState {
        self.policy_state.clone()
    }
}

impl BasicDecisionEngine {
    /// 计算所需权限等级
    fn calculate_required_privilege(&self, intent: &Intent) -> PrivilegeLevel {
        match &intent.proposed_action {
            Action::ReadFile { .. } => PrivilegeLevel::Normal,
            Action::WriteFile { path, .. } => {
                if Self::is_config_file(path) {
                    PrivilegeLevel::Elevated
                } else {
                    PrivilegeLevel::Normal
                }
            }
            Action::CreateFile { path, .. } => {
                if Self::is_config_file(path) {
                    PrivilegeLevel::Elevated
                } else {
                    PrivilegeLevel::Normal
                }
            }
            Action::DeleteFile { .. } => PrivilegeLevel::High,
            Action::RunCommand { command, .. } => {
                if Self::is_dangerous_command(command) {
                    PrivilegeLevel::Critical
                } else if Self::is_build_command(command) {
                    PrivilegeLevel::Elevated
                } else {
                    PrivilegeLevel::Normal
                }
            }
            Action::Git { operation, .. } => match operation {
                ndc_core::GitOp::Commit { .. } => PrivilegeLevel::High,
                ndc_core::GitOp::Push => PrivilegeLevel::High,
                _ => PrivilegeLevel::Normal,
            },
            Action::ModifyMemory { .. } => PrivilegeLevel::Elevated,
            Action::CreateTask { .. } => PrivilegeLevel::Normal,
            Action::UpdateTaskState { .. } => PrivilegeLevel::Normal,
            Action::SearchKnowledge { .. } => PrivilegeLevel::Normal,
            Action::SaveKnowledge { .. } => PrivilegeLevel::Elevated,
            Action::RunTests { .. } => PrivilegeLevel::Normal,
            Action::RunQualityCheck { .. } => PrivilegeLevel::Normal,
            Action::RequestHuman { .. } => PrivilegeLevel::Normal,
            Action::Other { .. } => PrivilegeLevel::Normal,
        }
    }

    /// 判断是否为配置文件
    fn is_config_file(path: &std::path::PathBuf) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains("Cargo.toml")
            || path_str.contains("package.json")
            || path_str.ends_with(".lock")
            || path_str.ends_with(".toml")
    }

    /// 判断是否为危险命令
    fn is_dangerous_command(command: &str) -> bool {
        let cmd = command.to_lowercase();
        cmd.contains("rm -rf") || cmd.contains("sudo") || cmd.contains(":(){:|:&};:")
    }

    /// 判断是否为构建命令
    fn is_build_command(command: &str) -> bool {
        let cmd = command.to_lowercase();
        cmd.contains("cargo build")
            || cmd.contains("npm run build")
            || cmd.contains("go build")
            || cmd.contains("make")
    }

    /// 构建附加条件
    fn build_conditions(&self, intent: &Intent) -> Vec<Condition> {
        let mut conditions = Vec::new();

        // 代码修改需要测试
        match intent.proposed_action {
            Action::WriteFile { .. } | Action::CreateFile { .. } | Action::DeleteFile { .. } => {
                conditions.push(Condition {
                    condition_type: ConditionType::MustPassTests,
                    description: "Code changes must pass tests".to_string(),
                });
            }
            _ => {}
        }

        conditions
    }
}
