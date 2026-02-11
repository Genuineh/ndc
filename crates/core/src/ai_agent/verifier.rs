//! Task Verifier - 任务完成验证与反馈循环
//!
//! 职责:
//! - 验证任务是否真正完成
//! - 生成继续指令
//! - 实现反馈循环
//!
//! 注意: 为了避免循环依赖，此模块使用 trait 抽象而不是直接依赖 runtime

use crate::{TaskId, TaskState, Action};
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
pub struct TaskVerifier {
    /// 任务存储
    storage: Arc<dyn TaskStorage>,

    /// 质量门禁 (可选)
    quality_gate: Option<Arc<dyn QualityGate>>,
}

impl TaskVerifier {
    /// 创建新的 Task Verifier
    pub fn new(storage: Arc<dyn TaskStorage>) -> Self {
        Self {
            storage,
            quality_gate: None,
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
        }
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
}

impl Clone for TaskVerifier {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            quality_gate: self.quality_gate.as_ref().map(Arc::clone),
        }
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
}
