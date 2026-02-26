//! 内置校验器
//!
//! - TaskBoundaryValidator: 确保 Action 不超出 Task 范围
//! - PermissionValidator: 确保 Agent 有执行权限
//! - SecurityPolicyValidator: 防止危险操作
//! - DependencyValidator: 确保前置条件满足

use crate::engine::{PolicyState, ValidationResult};
use async_trait::async_trait;
use ndc_core::{AgentRole, Intent};
use std::sync::Arc;

/// 校验器 Trait
#[async_trait]
pub trait Validator: Send + Sync + std::fmt::Debug {
    /// 校验 Intent
    async fn validate(&self, intent: &Intent, policy: &PolicyState) -> ValidationResult;
}

/// 任务边界校验器
#[derive(Debug, Default)]
pub struct TaskBoundaryValidator;

#[async_trait]
impl Validator for TaskBoundaryValidator {
    async fn validate(&self, intent: &Intent, _policy: &PolicyState) -> ValidationResult {
        // 如果没有关联任务，检查是否是创建任务的请求
        if intent.task_id.is_none() {
            match &intent.proposed_action {
                ndc_core::Action::CreateTask { .. } => {
                    // Planner 角色可以创建任务
                    if intent.agent_role == AgentRole::Planner {
                        return ValidationResult::Allow;
                    }
                }
                _ => {
                    return ValidationResult::Deny(
                        "Intent must be associated with a task".to_string(),
                    );
                }
            }
        }

        ValidationResult::Allow
    }
}

/// 权限校验器
#[derive(Debug, Default)]
pub struct PermissionValidator;

#[async_trait]
impl Validator for PermissionValidator {
    async fn validate(&self, intent: &Intent, _policy: &PolicyState) -> ValidationResult {
        // 检查角色是否有权限执行此操作
        match &intent.proposed_action {
            // 删除文件只有 Human 或特定角色可以做
            ndc_core::Action::DeleteFile { .. } => {
                if intent.agent_role != AgentRole::Admin {
                    return ValidationResult::RequireHuman(
                        "Delete file operation requires human approval".to_string(),
                        ndc_core::HumanContext {
                            task_id: intent.task_id,
                            affected_files: vec![],
                            risk_level: ndc_core::RiskLevel::Critical,
                            alternatives: vec![],
                            required_privilege: ndc_core::PrivilegeLevel::Critical,
                        },
                    );
                }
            }

            // 修改系统配置需要提升权限
            ndc_core::Action::WriteFile { path, .. } => {
                let is_config = path.to_string_lossy().contains("Cargo.toml")
                    || path.to_string_lossy().contains("package.json");
                if is_config && intent.agent_role != AgentRole::Admin {
                    return ValidationResult::RequireHuman(
                        "System configuration modification requires human approval".to_string(),
                        ndc_core::HumanContext {
                            task_id: intent.task_id,
                            affected_files: vec![path.clone()],
                            risk_level: ndc_core::RiskLevel::Medium,
                            alternatives: vec![],
                            required_privilege: ndc_core::PrivilegeLevel::Elevated,
                        },
                    );
                }
            }

            // 运行危险命令需要人类确认
            ndc_core::Action::RunCommand { command, .. } => {
                let is_dangerous = command.contains("rm -rf")
                    || command.contains("sudo")
                    || command.contains(":(){:|:&};:");
                if is_dangerous {
                    return ValidationResult::Deny("Dangerous command is not allowed".to_string());
                }
            }

            _ => {}
        }

        ValidationResult::Allow
    }
}

/// 安全策略校验器
#[derive(Debug, Default)]
pub struct SecurityPolicyValidator;

#[async_trait]
impl Validator for SecurityPolicyValidator {
    async fn validate(&self, intent: &Intent, policy: &PolicyState) -> ValidationResult {
        // 检查是否启用严格模式
        if policy.strict_mode {
            // 严格模式下禁止所有删除操作
            match &intent.proposed_action {
                ndc_core::Action::DeleteFile { .. } => {
                    return ValidationResult::Deny(
                        "Delete operations are not allowed in strict mode".to_string(),
                    );
                }
                ndc_core::Action::RunCommand { command, .. } => {
                    let cmd = command.to_lowercase();
                    if cmd.contains("rm") || cmd.contains("del") {
                        return ValidationResult::Deny(
                            "Delete commands are not allowed in strict mode".to_string(),
                        );
                    }
                }
                _ => {}
            }
        }

        // 检查是否允许危险操作
        if !policy.allow_dangerous
            && let ndc_core::Action::RunCommand { command, .. } = &intent.proposed_action
        {
            let cmd = command.to_lowercase();
            if cmd.contains("sudo") || cmd.contains("chmod 777") || cmd.contains("mkfs") {
                return ValidationResult::Deny("Dangerous operations are not allowed".to_string());
            }
        }

        ValidationResult::Allow
    }
}

/// 依赖校验器
#[derive(Debug, Default)]
pub struct DependencyValidator;

#[async_trait]
impl Validator for DependencyValidator {
    async fn validate(&self, intent: &Intent, _policy: &PolicyState) -> ValidationResult {
        // 检查状态转换的前置条件
        if let ndc_core::Action::UpdateTaskState {
            task_id, new_state, ..
        } = &intent.proposed_action
        {
            // 这里应该检查任务依赖是否满足
            // 目前简化处理
            tracing::debug!(
                "Checking state transition for task {}: {:?}",
                task_id,
                new_state
            );
        }

        ValidationResult::Allow
    }
}

/// 校验器注册表
#[derive(Debug, Default)]
pub struct ValidatorRegistry {
    validators: Vec<Arc<dyn Validator + Send + Sync>>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    pub fn register(&mut self, validator: Arc<dyn Validator + Send + Sync>) {
        self.validators.push(validator);
    }

    pub fn get_all(&self) -> Vec<Arc<dyn Validator + Send + Sync>> {
        self.validators.clone()
    }
}
