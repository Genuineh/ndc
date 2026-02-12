//! Task Verifier - 任务完成验证与反馈循环
//!
//! 职责:
//! - 验证任务是否真正完成
//! - 生成继续指令
//! - 实现反馈循环
//! - 集成 Knowledge Injectors (WorkingMemory, Invariants, Lineage)
//!
//! 注意: 为了避免循环依赖，此模块使用 trait 抽象而不是直接依赖 runtime

use crate::{TaskId, TaskState, Action};
use super::injectors::working_memory::WorkingMemoryInjector;
use super::injectors::invariant::{InvariantInjector, InvariantEntry, InvariantPriority};
use super::injectors::lineage::LineageInjector;
use std::sync::Arc;
use thiserror::Error;
use async_trait::async_trait;

/// 验证错误
#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Quality gate failed: {0}")]
    QualityGateFailed(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

/// 验证结果
#[derive(Debug, Clone)]
pub enum VerificationResult {
    /// 任务已完成
    Completed,

    /// 任务未完成
    Incomplete { reason: String },

    /// 质量门禁失败
    QualityGateFailed { reason: String },
}

impl VerificationResult {
    /// 是否成功
    pub fn is_success(&self) -> bool {
        matches!(self, VerificationResult::Completed)
    }

    /// 获取失败原因
    pub fn failure_reason(&self) -> Option<&String> {
        match self {
            VerificationResult::Incomplete { reason } => Some(reason),
            VerificationResult::QualityGateFailed { reason } => Some(reason),
            VerificationResult::Completed => None,
        }
    }
}

/// 任务存储抽象 (避免循环依赖)
#[async_trait]
pub trait TaskStorage: Send + Sync {
    async fn get_task(&self, id: &TaskId) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>>;
}

/// 质量门禁抽象
#[async_trait]
pub trait QualityGate: Send + Sync {
    async fn run(&self, gate_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Task Verifier
#[derive(Clone)]
pub struct TaskVerifier {
    /// 任务存储
    storage: Arc<dyn TaskStorage>,

    /// 质量门禁 (可选)
    quality_gate: Option<Arc<dyn QualityGate>>,

    /// Working Memory Injector (可选) - 用于记录失败模式
    working_memory: Option<WorkingMemoryInjector>,

    /// Invariant Injector (可选) - 用于从失败中学习
    invariants: Option<InvariantInjector>,

    /// Lineage Injector (可选) - 用于追踪验证历史
    lineage: Option<LineageInjector>,
}

impl TaskVerifier {
    /// 创建新的 Task Verifier
    pub fn new(storage: Arc<dyn TaskStorage>) -> Self {
        Self {
            storage,
            quality_gate: None,
            working_memory: None,
            invariants: None,
            lineage: None,
        }
    }

    /// 创建带质量门禁的 Task Verifier
    pub fn with_quality_gate(
        storage: Arc<dyn TaskStorage>,
        quality_gate: Arc<dyn QualityGate>,
    ) -> Self {
        Self {
            storage,
            quality_gate: Some(quality_gate),
            working_memory: None,
            invariants: None,
            lineage: None,
        }
    }

    /// 设置 Working Memory Injector
    pub fn with_working_memory(mut self, working_memory: WorkingMemoryInjector) -> Self {
        self.working_memory = Some(working_memory);
        self
    }

    /// 设置 Invariant Injector
    pub fn with_invariants(mut self, invariants: InvariantInjector) -> Self {
        self.invariants = Some(invariants);
        self
    }

    /// 设置 Lineage Injector
    pub fn with_lineage(mut self, lineage: LineageInjector) -> Self {
        self.lineage = Some(lineage);
        self
    }

    /// 验证任务是否真正完成
    pub async fn verify_completion(&self, task_id: &TaskId) -> Result<VerificationResult, VerificationError> {
        // 1. 获取任务
        let task = self.storage.get_task(task_id).await
            .map_err(|e| VerificationError::StorageError(e.to_string()))?
            .ok_or_else(|| VerificationError::TaskNotFound(*task_id))?;

        // 2. 检查任务状态
        if task.state != TaskState::Completed {
            return Ok(VerificationResult::Incomplete {
                reason: format!("Task is in {:?} state, not Completed", task.state),
            });
        }

        // 3. 验证执行步骤
        for step in &task.steps {
            if let Some(ref result) = step.result {
                if !result.success {
                    return Ok(VerificationResult::Incomplete {
                        reason: format!(
                            "Step {} ({}) failed: {}",
                            step.step_id,
                            format_action(&step.action),
                            result.error.as_ref().unwrap_or(&"Unknown error".to_string())
                        ),
                    });
                }
            }
        }

        // 4. 运行质量门禁 (如果配置了)
        if let (Some(gate), Some(quality_gate)) = (self.quality_gate.as_ref(), &task.quality_gate) {
            let gate_name = format!("{:?}", quality_gate);
            match gate.run(&gate_name).await {
                Ok(_) => {},
                Err(e) => {
                    return Ok(VerificationResult::QualityGateFailed {
                        reason: e.to_string(),
                    });
                }
            }
        }

        // 5. 验证通过
        Ok(VerificationResult::Completed)
    }

    /// 生成继续指令
    pub fn generate_continuation_prompt(&self, result: &VerificationResult) -> String {
        match result {
            VerificationResult::Completed => {
                "✅ Task verified as completed! Great work!".to_string()
            }
            VerificationResult::Incomplete { reason } => {
                format!(
                    "❌ Task verification failed:\n\n{}\n\n\
                     Please continue working on this task and address the issues above.\n\n\
                     When you believe the task is complete, submit it for verification again.",
                    reason
                )
            }
            VerificationResult::QualityGateFailed { reason } => {
                format!(
                    "❌ Quality gate failed:\n\n{}\n\n\
                     Please fix the issues and run the quality checks again.\n\n\
                     Use the 'run_tests' tool to verify your changes.",
                    reason
                )
            }
        }
    }

    /// 生成验证反馈消息
    pub fn generate_feedback_message(&self, result: &VerificationResult) -> String {
        match result {
            VerificationResult::Completed => {
                "✅ Task verified successfully! All checks passed.".to_string()
            }
            VerificationResult::Incomplete { reason } => {
                format!("⚠️ Task incomplete: {}", reason)
            }
            VerificationResult::QualityGateFailed { reason } => {
                format!("🚫 Quality gate failed: {}", reason)
            }
        }
    }

    /// 验证并记录到 Working Memory - 增强版
    pub async fn verify_and_track(&self, task_id: &TaskId) -> Result<VerificationResult, VerificationError> {
        let result = self.verify_completion(task_id).await?;
        Ok(result)
    }

    /// 从失败中提取 Invariant
    pub fn extract_invariant_from_failure(task_id: &TaskId, reason: &str) -> Option<InvariantEntry> {
        let description = if reason.contains("test") && reason.contains("fail") {
            Some("Tests failing indicates incomplete implementation or missing test coverage")
        } else if reason.contains("file") && reason.contains("not found") {
            Some("Missing files indicate incomplete file creation or incorrect paths")
        } else if reason.contains("state") && reason.contains("not Completed") {
            Some("Task was marked complete but not in Completed state")
        } else {
            None
        };

        description.map(|desc| {
            InvariantEntry::new(
                format!("auto-{}", task_id),
                desc.to_string(),
                InvariantPriority::Medium,
            )
        })
    }

    /// 获取失败原因用于 Working Memory 记录
    pub fn get_failure_for_tracking(&self, result: &VerificationResult) -> Option<String> {
        result.failure_reason().cloned()
    }

    /// 生成带知识注入的继续指令
    pub fn generate_enhanced_continuation(&self, result: &VerificationResult) -> String {
        let base_prompt = self.generate_continuation_prompt(result);

        // 添加 Working Memory 注入
        let wm_injection = self.working_memory.as_ref()
            .map(|wm| wm.inject())
            .unwrap_or_else(|| "(No working memory context)".to_string());

        // 添加 Invariant 提示
        let inv_hint = if let Some(ref inv) = self.invariants {
            let stats = inv.stats();
            if stats.total > 0 {
                format!("\n\n📋 Current invariants: {} active ({} critical, {} high, {} medium, {} low)",
                    stats.active, stats.critical, stats.high, stats.medium, stats.low)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        format!("{}\n\n{}\n{}", base_prompt, wm_injection, inv_hint)
    }
}

/// 格式化操作描述
fn format_action(action: &Action) -> String {
    match action {
        Action::ReadFile { path } => {
            format!("read file: {}", path.display())
        }
        Action::WriteFile { path, .. } => {
            format!("write file: {}", path.display())
        }
        Action::CreateFile { path } => {
            format!("create file: {}", path.display())
        }
        Action::DeleteFile { path } => {
            format!("delete file: {}", path.display())
        }
        Action::RunCommand { command, args } => {
            format!("run command: {} {}", command, args.join(" "))
        }
        Action::RunTests { test_type } => {
            format!("run tests: {:?}", test_type)
        }
        _ => {
            format!("action: {:?}", action)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Mock storage for testing
    struct MockStorage;

    #[async_trait]
    impl TaskStorage for MockStorage {
        async fn get_task(&self, _id: &TaskId) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }
    }

    #[test]
    fn test_verification_result_completed() {
        let result = VerificationResult::Completed;
        assert!(result.is_success());
        assert!(result.failure_reason().is_none());
    }

    #[test]
    fn test_verification_result_incomplete() {
        let result = VerificationResult::Incomplete {
            reason: "Tests failed".to_string(),
        };
        assert!(!result.is_success());
        assert_eq!(result.failure_reason(), Some(&"Tests failed".to_string()));
    }

    #[test]
    fn test_verification_result_quality_gate_failed() {
        let result = VerificationResult::QualityGateFailed {
            reason: "Clippy warnings".to_string(),
        };
        assert!(!result.is_success());
        assert_eq!(result.failure_reason(), Some(&"Clippy warnings".to_string()));
    }

    #[test]
    fn test_generate_continuation_prompt() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));

        let completed = VerificationResult::Completed;
        let prompt = verifier.generate_continuation_prompt(&completed);
        assert!(prompt.contains("verified"));
        assert!(prompt.contains("✅"));

        let incomplete = VerificationResult::Incomplete {
            reason: "File not found".to_string(),
        };
        let prompt = verifier.generate_continuation_prompt(&incomplete);
        assert!(prompt.contains("File not found"));
        assert!(prompt.contains("❌"));
    }

    #[test]
    fn test_generate_feedback_message() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));

        let completed = VerificationResult::Completed;
        let feedback = verifier.generate_feedback_message(&completed);
        assert!(feedback.contains("verified"));
        assert!(feedback.contains("✅"));

        let failed = VerificationResult::QualityGateFailed {
            reason: "Tests failed".to_string(),
        };
        let feedback = verifier.generate_feedback_message(&failed);
        assert!(feedback.contains("Tests failed"));
        assert!(feedback.contains("🚫"));
    }

    #[test]
    fn test_format_action() {
        let action = Action::ReadFile {
            path: std::path::PathBuf::from("test.rs"),
        };
        let formatted = format_action(&action);
        assert!(formatted.contains("read file"));
        assert!(formatted.contains("test.rs"));

        let action = Action::RunCommand {
            command: "cargo".to_string(),
            args: vec!["test".to_string()],
        };
        let formatted = format_action(&action);
        assert!(formatted.contains("run command"));
        assert!(formatted.contains("cargo test"));

        let action = Action::WriteFile {
            path: std::path::PathBuf::from("output.rs"),
            content: "content".to_string(),
        };
        let formatted = format_action(&action);
        assert!(formatted.contains("write file"));
        assert!(formatted.contains("output.rs"));
    }

    #[test]
    fn test_task_verifier_new() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));
        // Should create without error
        assert!(verifier.quality_gate.is_none());
    }

    #[test]
    fn test_task_verifier_clone() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));
        let cloned = verifier.clone();
        // Both should have the same storage reference
        assert!(Arc::ptr_eq(&verifier.storage, &cloned.storage));
    }

    #[test]
    fn test_extract_invariant_from_failure() {
        let task_id = TaskId::new();

        // Test test failure pattern
        let result = VerificationResult::Incomplete {
            reason: "test failed with error".to_string(),
        };
        let invariant = TaskVerifier::extract_invariant_from_failure(&task_id, result.failure_reason().unwrap());
        assert!(invariant.is_some());
        assert!(invariant.unwrap().description.contains("incomplete"));

        // Test no pattern match
        let result2 = VerificationResult::Incomplete {
            reason: "some other issue".to_string(),
        };
        let invariant2 = TaskVerifier::extract_invariant_from_failure(&task_id, result2.failure_reason().unwrap());
        assert!(invariant2.is_none());
    }

    #[test]
    fn test_get_failure_for_tracking() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));

        let failed = VerificationResult::Incomplete {
            reason: "Tests failed".to_string(),
        };
        assert_eq!(verifier.get_failure_for_tracking(&failed), Some("Tests failed".to_string()));

        let completed = VerificationResult::Completed;
        assert!(verifier.get_failure_for_tracking(&completed).is_none());
    }

    #[test]
    fn test_generate_enhanced_continuation() {
        use crate::ai_agent::injectors::invariant::{InvariantInjector, InvariantEntry, InvariantPriority};

        let verifier = TaskVerifier::new(Arc::new(MockStorage));

        // Add invariants
        let mut inv = InvariantInjector::default();
        inv.add_invariant(InvariantEntry::new(
            "test".to_string(),
            "Test invariant".to_string(),
            InvariantPriority::High,
        ));

        let verifier_with_inv = verifier.with_invariants(inv);
        let result = VerificationResult::Incomplete {
            reason: "Test failed".to_string(),
        };

        let enhanced = verifier_with_inv.generate_enhanced_continuation(&result);
        assert!(enhanced.contains("Current invariants"));
        assert!(enhanced.contains("1 active"));
        assert!(enhanced.contains("invariants") || enhanced.contains("INVARIANTS"));
    }
}
