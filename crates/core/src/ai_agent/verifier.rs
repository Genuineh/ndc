//! Task Verifier - 任务完成验证与反馈循环
//!
//! 职责:
//! - 验证任务是否真正完成
//! - 生成继续指令
//! - 实现反馈循环
//! - 集成 Knowledge Injectors (WorkingMemory, Invariants, Lineage)
//!
//! 注意: 为了避免循环依赖，此模块使用 trait 抽象而不是直接依赖 runtime

use super::injectors::invariant::{InvariantEntry, InvariantInjector, InvariantPriority};
use super::injectors::lineage::LineageInjector;
use super::injectors::working_memory::WorkingMemoryInjector;
use crate::{
    AccessControl, Action, AgentId, MemoryContent, MemoryEntry, MemoryId, MemoryMetadata,
    MemoryStability, SystemFactInput, TaskId, TaskState,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;

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
    async fn get_task(
        &self,
        id: &TaskId,
    ) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>>;
    async fn save_memory(
        &self,
        memory: &MemoryEntry,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get_memory(
        &self,
        id: &MemoryId,
    ) -> Result<Option<MemoryEntry>, Box<dyn std::error::Error + Send + Sync>>;
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

    /// Gold memory service for invariant feedback loop
    gold_memory: Option<Arc<Mutex<crate::GoldMemoryService>>>,

    /// Task -> created invariant IDs for validation tracking
    tracked_invariants: Arc<Mutex<HashMap<TaskId, Vec<crate::InvariantId>>>>,

    /// Whether persisted gold memory has been loaded from storage
    gold_memory_loaded: Arc<Mutex<bool>>,

    /// Whether current in-memory state originated from a v1 payload and needs migration audit.
    migrate_from_v1_pending: Arc<Mutex<bool>>,
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
            gold_memory: None,
            tracked_invariants: Arc::new(Mutex::new(HashMap::new())),
            gold_memory_loaded: Arc::new(Mutex::new(false)),
            migrate_from_v1_pending: Arc::new(Mutex::new(false)),
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
            gold_memory: None,
            tracked_invariants: Arc::new(Mutex::new(HashMap::new())),
            gold_memory_loaded: Arc::new(Mutex::new(false)),
            migrate_from_v1_pending: Arc::new(Mutex::new(false)),
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

    /// Enable GoldMemory feedback loop
    pub fn with_gold_memory(mut self, gold_memory: Arc<Mutex<crate::GoldMemoryService>>) -> Self {
        self.gold_memory = Some(gold_memory);
        self
    }

    /// 验证任务是否真正完成
    pub async fn verify_completion(
        &self,
        task_id: &TaskId,
    ) -> Result<VerificationResult, VerificationError> {
        // 1. 获取任务
        let task = self
            .storage
            .get_task(task_id)
            .await
            .map_err(|e| VerificationError::StorageError(e.to_string()))?
            .ok_or(VerificationError::TaskNotFound(*task_id))?;

        // 2. 检查任务状态
        if task.state != TaskState::Completed {
            return Ok(VerificationResult::Incomplete {
                reason: format!("Task is in {:?} state, not Completed", task.state),
            });
        }

        // 3. 验证执行步骤
        for step in &task.steps {
            if let Some(ref result) = step.result
                && !result.success
            {
                return Ok(VerificationResult::Incomplete {
                    reason: format!(
                        "Step {} ({}) failed: {}",
                        step.step_id,
                        format_action(&step.action),
                        result
                            .error
                            .as_ref()
                            .unwrap_or(&"Unknown error".to_string())
                    ),
                });
            }
        }

        // 4. 运行质量门禁 (如果配置了)
        if let (Some(gate), Some(quality_gate)) = (self.quality_gate.as_ref(), &task.quality_gate) {
            let gate_name = format!("{:?}", quality_gate);
            match gate.run(&gate_name).await {
                Ok(_) => {}
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
    pub async fn verify_and_track(
        &self,
        task_id: &TaskId,
    ) -> Result<VerificationResult, VerificationError> {
        self.ensure_gold_memory_loaded().await?;
        let result = self.verify_completion(task_id).await?;
        self.update_gold_memory_feedback(task_id, &result)?;
        self.persist_gold_memory(task_id).await?;
        Ok(result)
    }

    fn gold_memory_entry_id() -> Result<MemoryId, VerificationError> {
        let uuid = uuid::Uuid::parse_str("00000000-0000-0000-0000-00000000a801")
            .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
        Ok(MemoryId(uuid))
    }

    async fn ensure_gold_memory_loaded(&self) -> Result<(), VerificationError> {
        let Some(gold_memory) = &self.gold_memory else {
            return Ok(());
        };

        {
            let loaded = self
                .gold_memory_loaded
                .lock()
                .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
            if *loaded {
                return Ok(());
            }
        }

        let entry_id = Self::gold_memory_entry_id()?;
        if let Some(entry) = self
            .storage
            .get_memory(&entry_id)
            .await
            .map_err(|e| VerificationError::StorageError(e.to_string()))?
            && let Some((service, migrated_from_v1)) = Self::decode_gold_memory_entry(&entry)?
        {
            let mut gm = gold_memory
                .lock()
                .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
            *gm = service;
            let mut pending = self
                .migrate_from_v1_pending
                .lock()
                .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
            *pending = migrated_from_v1;
        }

        let mut loaded = self
            .gold_memory_loaded
            .lock()
            .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
        *loaded = true;
        Ok(())
    }

    async fn persist_gold_memory(&self, task_id: &TaskId) -> Result<(), VerificationError> {
        let Some(gold_memory) = &self.gold_memory else {
            return Ok(());
        };

        let migration = {
            let mut pending = self
                .migrate_from_v1_pending
                .lock()
                .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
            let migration = if *pending {
                Some(MigrationAuditV2 {
                    from_version: 1,
                    migrated_at: chrono::Utc::now(),
                    trigger_task_id: task_id.to_string(),
                    trigger_source: "task_verifier".to_string(),
                })
            } else {
                None
            };
            *pending = false;
            migration
        };

        let service = gold_memory
            .lock()
            .map_err(|e| VerificationError::ExecutionError(e.to_string()))?
            .clone();
        let payload = serde_json::to_string(&PersistedGoldMemoryV2 {
            version: 2,
            service: serde_json::to_value(&service)
                .map_err(|e| VerificationError::ExecutionError(e.to_string()))?,
            migration,
        })
        .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
        let entry = MemoryEntry {
            id: Self::gold_memory_entry_id()?,
            content: MemoryContent::General {
                text: payload,
                metadata: "gold_memory_service/v2".to_string(),
            },
            embedding: Vec::new(),
            relations: Vec::new(),
            metadata: MemoryMetadata {
                stability: MemoryStability::Canonical,
                created_at: chrono::Utc::now(),
                created_by: AgentId::system(),
                source_task: *task_id,
                version: 1,
                modified_at: Some(chrono::Utc::now()),
                tags: vec!["gold-memory".to_string(), "invariants".to_string()],
            },
            access_control: AccessControl::new(AgentId::system(), MemoryStability::Canonical),
        };
        self.storage
            .save_memory(&entry)
            .await
            .map_err(|e| VerificationError::StorageError(e.to_string()))
    }

    fn decode_gold_memory_entry(
        entry: &MemoryEntry,
    ) -> Result<Option<(crate::GoldMemoryService, bool)>, VerificationError> {
        match &entry.content {
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v2" => {
                let persisted: PersistedGoldMemoryV2 = serde_json::from_str(text)
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                let service: crate::GoldMemoryService =
                    serde_json::from_value(persisted.service)
                        .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                Ok(Some((service, false)))
            }
            // Legacy payload compatibility (v1 stored raw GoldMemoryService)
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v1" => {
                let service: crate::GoldMemoryService = serde_json::from_str(text)
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                Ok(Some((service, true)))
            }
            _ => Ok(None),
        }
    }

    fn update_gold_memory_feedback(
        &self,
        task_id: &TaskId,
        result: &VerificationResult,
    ) -> Result<(), VerificationError> {
        let Some(gold_memory) = &self.gold_memory else {
            return Ok(());
        };

        match result {
            VerificationResult::Completed => {
                let tracked = self
                    .tracked_invariants
                    .lock()
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?
                    .get(task_id)
                    .cloned()
                    .unwrap_or_default();

                let mut service = gold_memory
                    .lock()
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                for id in tracked {
                    service.mark_validated(&id);
                }
                Ok(())
            }
            VerificationResult::Incomplete { reason }
            | VerificationResult::QualityGateFailed { reason } => {
                let mut service = gold_memory
                    .lock()
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                let fact = Self::structured_fact(task_id, result, reason);
                let upserted = service.upsert_system_fact(SystemFactInput {
                    dedupe_key: Self::fact_dedupe_key(task_id, &fact.kind),
                    rule: fact.rule,
                    description: fact.description,
                    scope_pattern: task_id.to_string(),
                    priority: fact.priority,
                    tags: fact.tags,
                    evidence: fact.evidence,
                    source: "verifier".to_string(),
                });
                service.mark_violated(&upserted.id);
                drop(service);

                let mut tracked = self
                    .tracked_invariants
                    .lock()
                    .map_err(|e| VerificationError::ExecutionError(e.to_string()))?;
                let entry = tracked.entry(*task_id).or_default();
                if !entry.contains(&upserted.id) {
                    entry.push(upserted.id);
                }
                Ok(())
            }
        }
    }

    fn fact_dedupe_key(task_id: &TaskId, kind: &str) -> String {
        format!("task:{}:{}", task_id, kind.to_ascii_lowercase())
    }

    fn structured_fact(
        task_id: &TaskId,
        result: &VerificationResult,
        reason: &str,
    ) -> StructuredFact {
        let lower = reason.to_ascii_lowercase();
        match result {
            VerificationResult::QualityGateFailed { .. } => StructuredFact {
                rule: format!(
                    "Quality gate must pass before task {} can complete",
                    task_id
                ),
                description: format!("Quality gate failure detected: {}", reason),
                priority: crate::InvariantPriority::Critical,
                tags: vec![
                    "verification".to_string(),
                    "quality_gate".to_string(),
                    "regression_risk".to_string(),
                ],
                kind: "quality_gate_failed".to_string(),
                evidence: vec![
                    format!("task_id={}", task_id),
                    "kind=quality_gate_failed".to_string(),
                    format!("reason={}", reason),
                ],
            },
            VerificationResult::Incomplete { .. }
                if lower.contains("not completed") || lower.contains("state") =>
            {
                StructuredFact {
                    rule: format!(
                        "Task {} must be in Completed state before finalize",
                        task_id
                    ),
                    description: format!("Task state validation failed: {}", reason),
                    priority: crate::InvariantPriority::High,
                    tags: vec!["verification".to_string(), "task_state".to_string()],
                    kind: "state_incomplete".to_string(),
                    evidence: vec![
                        format!("task_id={}", task_id),
                        "kind=state_incomplete".to_string(),
                        format!("reason={}", reason),
                    ],
                }
            }
            VerificationResult::Incomplete { .. }
                if lower.contains("step") && lower.contains("failed") =>
            {
                StructuredFact {
                    rule: format!("All execution steps for task {} must succeed", task_id),
                    description: format!("Execution step failed during verification: {}", reason),
                    priority: crate::InvariantPriority::High,
                    tags: vec!["verification".to_string(), "execution_step".to_string()],
                    kind: "step_failure".to_string(),
                    evidence: vec![
                        format!("task_id={}", task_id),
                        "kind=step_failure".to_string(),
                        format!("reason={}", reason),
                    ],
                }
            }
            _ => StructuredFact {
                rule: format!(
                    "Verification must pass for task {} before completion",
                    task_id
                ),
                description: format!("Verification incomplete: {}", reason),
                priority: crate::InvariantPriority::Medium,
                tags: vec!["verification".to_string(), "incomplete".to_string()],
                kind: "verification_incomplete".to_string(),
                evidence: vec![
                    format!("task_id={}", task_id),
                    "kind=verification_incomplete".to_string(),
                    format!("reason={}", reason),
                ],
            },
        }
    }

    /// Read-only summary for observability/tests
    pub fn gold_memory_summary(&self) -> Option<crate::GoldMemorySummary> {
        self.gold_memory
            .as_ref()
            .and_then(|gm| gm.lock().ok().map(|service| service.summary()))
    }

    /// 从失败中提取 Invariant
    pub fn extract_invariant_from_failure(
        task_id: &TaskId,
        reason: &str,
    ) -> Option<InvariantEntry> {
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
        let wm_injection = self
            .working_memory
            .as_ref()
            .map(|wm| wm.inject())
            .unwrap_or_else(|| "(No working memory context)".to_string());

        // 添加 Invariant 提示
        let inv_hint = if let Some(ref inv) = self.invariants {
            let stats = inv.stats();
            if stats.total > 0 {
                format!(
                    "\n\n📋 Current invariants: {} active ({} critical, {} high, {} medium, {} low)",
                    stats.active, stats.critical, stats.high, stats.medium, stats.low
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        format!("{}\n\n{}\n{}", base_prompt, wm_injection, inv_hint)
    }
}

struct StructuredFact {
    rule: String,
    description: String,
    priority: crate::InvariantPriority,
    tags: Vec<String>,
    kind: String,
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedGoldMemoryV2 {
    version: u32,
    service: serde_json::Value,
    migration: Option<MigrationAuditV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationAuditV2 {
    from_version: u32,
    migrated_at: chrono::DateTime<chrono::Utc>,
    trigger_task_id: String,
    trigger_source: String,
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
    use std::collections::HashMap as StdHashMap;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;

    // Mock storage for testing
    struct MockStorage;

    #[async_trait]
    impl TaskStorage for MockStorage {
        async fn get_task(
            &self,
            _id: &TaskId,
        ) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn save_memory(
            &self,
            _memory: &MemoryEntry,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }

        async fn get_memory(
            &self,
            _id: &MemoryId,
        ) -> Result<Option<MemoryEntry>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }
    }

    struct StatefulStorage {
        task: StdMutex<crate::Task>,
        memories: StdMutex<StdHashMap<MemoryId, MemoryEntry>>,
    }

    #[async_trait]
    impl TaskStorage for StatefulStorage {
        async fn get_task(
            &self,
            id: &TaskId,
        ) -> Result<Option<crate::Task>, Box<dyn std::error::Error + Send + Sync>> {
            let task = self.task.lock().unwrap().clone();
            if &task.id == id {
                Ok(Some(task))
            } else {
                Ok(None)
            }
        }

        async fn save_memory(
            &self,
            memory: &MemoryEntry,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.memories
                .lock()
                .unwrap()
                .insert(memory.id, memory.clone());
            Ok(())
        }

        async fn get_memory(
            &self,
            id: &MemoryId,
        ) -> Result<Option<MemoryEntry>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.memories.lock().unwrap().get(id).cloned())
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
        assert_eq!(
            result.failure_reason(),
            Some(&"Clippy warnings".to_string())
        );
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
        let invariant = TaskVerifier::extract_invariant_from_failure(
            &task_id,
            result.failure_reason().unwrap(),
        );
        assert!(invariant.is_some());
        assert!(invariant.unwrap().description.contains("incomplete"));

        // Test no pattern match
        let result2 = VerificationResult::Incomplete {
            reason: "some other issue".to_string(),
        };
        let invariant2 = TaskVerifier::extract_invariant_from_failure(
            &task_id,
            result2.failure_reason().unwrap(),
        );
        assert!(invariant2.is_none());
    }

    #[test]
    fn test_get_failure_for_tracking() {
        let verifier = TaskVerifier::new(Arc::new(MockStorage));

        let failed = VerificationResult::Incomplete {
            reason: "Tests failed".to_string(),
        };
        assert_eq!(
            verifier.get_failure_for_tracking(&failed),
            Some("Tests failed".to_string())
        );

        let completed = VerificationResult::Completed;
        assert!(verifier.get_failure_for_tracking(&completed).is_none());
    }

    #[test]
    fn test_generate_enhanced_continuation() {
        use crate::ai_agent::injectors::invariant::{
            InvariantEntry, InvariantInjector, InvariantPriority,
        };

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

    #[tokio::test]
    async fn test_verify_and_track_gold_memory_feedback_loop() {
        let mut task = crate::Task::new(
            "feedback loop".to_string(),
            "verify and track".to_string(),
            crate::AgentRole::Implementer,
        );
        let task_id = task.id;
        let storage = Arc::new(StatefulStorage {
            task: StdMutex::new(task.clone()),
            memories: StdMutex::new(StdHashMap::new()),
        });
        let gold = Arc::new(StdMutex::new(crate::GoldMemoryService::new()));

        let verifier = TaskVerifier::new(storage.clone()).with_gold_memory(gold.clone());

        let first = verifier.verify_and_track(&task_id).await.unwrap();
        assert!(matches!(first, VerificationResult::Incomplete { .. }));
        let summary_after_failure = verifier.gold_memory_summary().unwrap();
        assert_eq!(summary_after_failure.total_invariants, 1);
        assert_eq!(summary_after_failure.total_violations, 1);

        let second_failure = verifier.verify_and_track(&task_id).await.unwrap();
        assert!(matches!(
            second_failure,
            VerificationResult::Incomplete { .. }
        ));
        let summary_after_second_failure = verifier.gold_memory_summary().unwrap();
        assert_eq!(summary_after_second_failure.total_invariants, 1);
        assert!(summary_after_second_failure.total_violations >= 2);

        task.state = crate::TaskState::Completed;
        *storage.task.lock().unwrap() = task;

        let third = verifier.verify_and_track(&task_id).await.unwrap();
        assert!(matches!(third, VerificationResult::Completed));
        let summary_after_success = verifier.gold_memory_summary().unwrap();
        assert!(summary_after_success.total_validations >= 1);
    }

    #[tokio::test]
    async fn test_gold_memory_persists_across_verifier_instances() {
        let mut task = crate::Task::new(
            "persisted gold memory".to_string(),
            "should survive new verifier".to_string(),
            crate::AgentRole::Implementer,
        );
        let task_id = task.id;
        let storage = Arc::new(StatefulStorage {
            task: StdMutex::new(task.clone()),
            memories: StdMutex::new(StdHashMap::new()),
        });

        let first = TaskVerifier::new(storage.clone())
            .with_gold_memory(Arc::new(StdMutex::new(crate::GoldMemoryService::new())));
        let first_result = first.verify_and_track(&task_id).await.unwrap();
        assert!(matches!(
            first_result,
            VerificationResult::Incomplete { .. }
        ));
        assert_eq!(first.gold_memory_summary().unwrap().total_invariants, 1);

        task.state = crate::TaskState::Completed;
        *storage.task.lock().unwrap() = task;

        let second = TaskVerifier::new(storage.clone())
            .with_gold_memory(Arc::new(StdMutex::new(crate::GoldMemoryService::new())));
        let second_result = second.verify_and_track(&task_id).await.unwrap();
        assert!(matches!(second_result, VerificationResult::Completed));
        assert!(second.gold_memory_summary().unwrap().total_invariants >= 1);
    }

    #[tokio::test]
    async fn test_gold_memory_v1_migrates_to_v2_on_persist() {
        let task = crate::Task::new(
            "schema migration".to_string(),
            "v1 to v2".to_string(),
            crate::AgentRole::Implementer,
        );
        let task_id = task.id;
        let storage = Arc::new(StatefulStorage {
            task: StdMutex::new(task),
            memories: StdMutex::new(StdHashMap::new()),
        });

        let mut legacy_service = crate::GoldMemoryService::new();
        legacy_service.create_from_human_correction(
            "legacy".to_string(),
            "legacy error".to_string(),
            "legacy fix".to_string(),
            crate::InvariantContext {
                task_description: task_id.to_string(),
                files: Vec::new(),
                modules: Vec::new(),
                api_calls: Vec::new(),
                min_priority: Some(crate::InvariantPriority::Medium),
            },
        );
        let legacy_payload = serde_json::to_string(&legacy_service).unwrap();
        let legacy_entry = MemoryEntry {
            id: TaskVerifier::gold_memory_entry_id().unwrap(),
            content: MemoryContent::General {
                text: legacy_payload,
                metadata: "gold_memory_service/v1".to_string(),
            },
            embedding: Vec::new(),
            relations: Vec::new(),
            metadata: MemoryMetadata {
                stability: MemoryStability::Canonical,
                created_at: chrono::Utc::now(),
                created_by: AgentId::system(),
                source_task: task_id,
                version: 1,
                modified_at: Some(chrono::Utc::now()),
                tags: vec!["gold-memory".to_string()],
            },
            access_control: AccessControl::new(AgentId::system(), MemoryStability::Canonical),
        };
        storage
            .memories
            .lock()
            .unwrap()
            .insert(legacy_entry.id, legacy_entry);

        let verifier = TaskVerifier::new(storage.clone())
            .with_gold_memory(Arc::new(StdMutex::new(crate::GoldMemoryService::new())));
        let _ = verifier.verify_and_track(&task_id).await.unwrap();

        let stored = storage
            .memories
            .lock()
            .unwrap()
            .get(&TaskVerifier::gold_memory_entry_id().unwrap())
            .cloned()
            .unwrap();
        match stored.content {
            MemoryContent::General { text, metadata } => {
                assert_eq!(metadata, "gold_memory_service/v2");
                let payload: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(payload.get("version").and_then(|v| v.as_u64()), Some(2));
                let migration = payload.get("migration").cloned().unwrap_or_default();
                assert_eq!(
                    migration.get("from_version").and_then(|v| v.as_u64()),
                    Some(1)
                );
                assert_eq!(
                    migration
                        .get("trigger_task_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    task_id.to_string()
                );
            }
            _ => panic!("expected general memory entry"),
        }
    }
}
