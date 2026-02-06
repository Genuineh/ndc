//! Stability Manager - 记忆稳定性管理
//!
//! 职责：
//! - 记忆稳定性演化
//! - 稳定性规则引擎
//! - 自动升级/降级策略
//!
//! 稳定性层级：
//! - Ephemeral: 临时推理（随 Task 销毁）
//! - Derived: 推导结论（Reviewer 确认后升级）
//! - Verified: 已验证（测试/人类确认）
//! - Canonical: 典范（系统级真理，手动维护）

use ndc_core::{Memory, MemoryId, MemoryStability, StabilityChange, Timestamp};
use crate::memory::MemoryStore;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 稳定性管理错误
#[derive(Debug, Error)]
pub enum StabilityError {
    #[error("无法降级: {0}")]
    CannotDemote(String),

    #[error("升级失败: {0}")]
    UpgradeFailed(String),

    #[error("规则执行失败: {0}")]
    RuleExecutionFailed(String),
}

/// 稳定性变更规则
#[derive(Debug, Clone)]
pub struct StabilityRule {
    /// 规则名称
    pub name: String,

    /// 触发条件
    pub condition: StabilityCondition,

    /// 目标稳定性
    pub target_stability: MemoryStability,

    /// 是否自动执行
    pub automatic: bool,
}

#[derive(Debug, Clone)]
pub enum StabilityCondition {
    /// 经过 Reviewer 确认
    Reviewed { approved_by: Vec<String> },

    /// 测试通过
    TestsPassed { test_type: String },

    /// 人类确认
    HumanApproved,

    /// 时间阈值（秒）
    TimeElapsed { seconds: u64 },

    /// 访问次数阈值
    AccessCount { min_count: u32 },

    /// 自定义条件
    Custom(String),
}

/// 稳定性策略配置
#[derive(Debug, Clone)]
pub struct StabilityConfig {
    /// 是否启用自动升级
    pub auto_upgrade: bool,

    /// 是否启用自动清理
    pub auto_cleanup: bool,

    /// 临时记忆保留时间（秒）
    pub ephemeral_ttl: u64,

    /// 清理前确认次数
    pub confirm_before_cleanup: u32,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            auto_upgrade: true,
            auto_cleanup: true,
            ephemeral_ttl: 3600,      // 1 小时
            confirm_before_cleanup: 3,
        }
    }
}

/// 稳定性管理器
#[derive(Debug)]
pub struct StabilityManager {
    /// 记忆存储
    memory_store: Arc<MemoryStore>,

    /// 配置
    config: StabilityConfig,

    /// 升级规则
    upgrade_rules: Vec<StabilityRule>,
}

impl StabilityManager {
    /// 创建新的稳定性管理器
    pub fn new(memory_store: Arc<MemoryStore>, config: StabilityConfig) -> Self {
        let mut manager = Self {
            memory_store,
            config,
            upgrade_rules: Vec::new(),
        };

        // 注册默认规则
        manager.register_default_rules();

        manager
    }

    /// 创建默认配置的管理器
    pub fn default(memory_store: Arc<MemoryStore>) -> Self {
        Self::new(memory_store, StabilityConfig::default())
    }

    /// 注册默认规则
    fn register_default_rules(&mut self) {
        self.upgrade_rules = vec![
            // Ephemeral -> Derived: 被引用 3 次以上
            StabilityRule {
                name: "frequently_accessed".to_string(),
                condition: StabilityCondition::AccessCount { min_count: 3 },
                target_stability: MemoryStability::Derived,
                automatic: true,
            },

            // Ephemeral -> Derived: 超过 TTL
            StabilityRule {
                name: "time_elapsed".to_string(),
                condition: StabilityCondition::TimeElapsed {
                    seconds: 3600,  // 1 小时
                },
                target_stability: MemoryStability::Derived,
                automatic: true,
            },

            // Derived -> Verified: 测试通过
            StabilityRule {
                name: "tests_passed".to_string(),
                condition: StabilityCondition::TestsPassed {
                    test_type: "all".to_string(),
                },
                target_stability: MemoryStability::Verified,
                automatic: false,
            },

            // Derived -> Verified: 人类批准
            StabilityRule {
                name: "human_approved".to_string(),
                condition: StabilityCondition::HumanApproved,
                target_stability: MemoryStability::Verified,
                automatic: false,
            },

            // Verified -> Canonical: 手动升级
            StabilityRule {
                name: "canonical_manual".to_string(),
                condition: StabilityCondition::Custom("manual_promotion".to_string()),
                target_stability: MemoryStability::Canonical,
                automatic: false,
            },
        ];
    }

    /// 注册升级规则
    pub fn register_rule(&mut self, rule: StabilityRule) {
        self.upgrade_rules.push(rule);
    }

    /// 检查稳定性变更
    pub async fn check_stability(&self, memory_id: &MemoryId) -> Result<(), StabilityError> {
        let memory = self.memory_store.get(memory_id).await
            .map_err(|e| StabilityError::UpgradeFailed(e.to_string()))?
            .ok_or_else(|| StabilityError::UpgradeFailed("Memory not found".to_string()))?;

        // 检查是否可以升级
        if self.config.auto_upgrade {
            for rule in &self.upgrade_rules {
                if self.matches_condition(&rule.condition, &memory).await {
                    if rule.automatic {
                        self.upgrade(memory_id, rule.target_stability, &rule.name).await?;
                    }
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// 匹配条件
    async fn matches_condition(&self, condition: &StabilityCondition, memory: &Memory) -> bool {
        match condition {
            StabilityCondition::Reviewed { approved_by } => {
                // 检查是否有批准记录
                !memory.content.history.iter().any(|h| {
                    h.reason.contains("reviewed") ||
                    approved_by.iter().any(|a| h.reason.contains(a))
                })
            }
            StabilityCondition::TestsPassed { test_type } => {
                // 检查测试通过记录
                memory.content.history.iter().any(|h| {
                    h.reason.contains("test") &&
                    (test_type == "all" || h.reason.contains(test_type))
                })
            }
            StabilityCondition::HumanApproved => {
                // 检查人类批准
                memory.content.history.iter().any(|h| {
                    h.reason.contains("human") || h.reason.contains("approved")
                })
            }
            StabilityCondition::TimeElapsed { seconds } => {
                // 检查时间
                let elapsed = Timestamp::now()
                    .signed_duration_since(memory.content.last_accessed)
                    .num_seconds() as u64;
                elapsed >= *seconds
            }
            StabilityCondition::AccessCount { min_count } => {
                // 检查访问次数
                memory.content.access_count >= *min_count
            }
            StabilityCondition::Custom(_) => false,
        }
    }

    /// 升级稳定性
    pub async fn upgrade(
        &self,
        memory_id: &MemoryId,
        to_stability: MemoryStability,
        reason: &str,
    ) -> Result<(), StabilityError> {
        self.memory_store.promote(memory_id, to_stability, reason).await
            .map_err(|e| StabilityError::UpgradeFailed(e.to_string()))?;

        info!("Memory {} upgraded to {:?}", memory_id, to_stability);

        Ok(())
    }

    /// 降级稳定性（谨慎使用）
    pub async fn downgrade(
        &self,
        memory_id: &MemoryId,
        to_stability: MemoryStability,
        reason: &str,
    ) -> Result<(), StabilityError> {
        let memory = self.memory_store.get(memory_id).await
            .map_err(|e| StabilityError::CannotDemote(e.to_string()))?
            .ok_or_else(|| StabilityError::CannotDemote("Memory not found".to_string()))?;

        if memory.content.stability <= to_stability {
            return Err(StabilityError::CannotDemote(
                "Cannot demote to equal or higher stability".to_string(),
            ));
        }

        self.memory_store.promote(memory_id, to_stability, reason).await
            .map_err(|e| StabilityError::CannotDemote(e.to_string()))?;

        warn!("Memory {} downgraded to {:?}", memory_id, to_stability);

        Ok(())
    }

    /// 清理临时记忆
    pub async fn cleanup_ephemeral(&self) -> Result<u32, StabilityError> {
        if !self.config.auto_cleanup {
            return Ok(0);
        }

        let mut cleaned = 0;

        // 获取所有临时记忆
        let ephemeral_memories = self.memory_store.get_by_stability(
            MemoryStability::Ephemeral,
        ).await
            .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;

        for memory in ephemeral_memories {
            // 检查是否应该清理
            if self.should_cleanup(&memory).await {
                self.memory_store.delete(&memory.id).await
                    .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;
                cleaned += 1;

                debug!("Cleaned up ephemeral memory: {}", memory.id);
            }
        }

        info!("Cleaned up {} ephemeral memories", cleaned);

        Ok(cleaned)
    }

    /// 检查是否应该清理
    async fn should_cleanup(&self, memory: &Memory) -> bool {
        // 检查 TTL
        let elapsed = Timestamp::now()
            .signed_duration_since(memory.content.last_accessed)
            .num_seconds() as u64;

        if elapsed < self.config.ephemeral_ttl {
            return false;
        }

        // 检查确认次数
        if memory.content.access_count < self.config.confirm_before_cleanup {
            return false;
        }

        true
    }

    /// 获取稳定性统计
    pub async fn get_stats(&self) -> Result<StabilityStats, StabilityError> {
        let ephemeral = self.memory_store.get_by_stability(
            MemoryStability::Ephemeral,
        ).await
            .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;

        let derived = self.memory_store.get_by_stability(
            MemoryStability::Derived,
        ).await
            .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;

        let verified = self.memory_store.get_by_stability(
            MemoryStability::Verified,
        ).await
            .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;

        let canonical = self.memory_store.get_by_stability(
            MemoryStability::Canonical,
        ).await
            .map_err(|e| StabilityError::RuleExecutionFailed(e.to_string()))?;

        Ok(StabilityStats {
            ephemeral_count: ephemeral.len() as u64,
            derived_count: derived.len() as u64,
            verified_count: verified.len() as u64,
            canonical_count: canonical.len() as u64,
            total_count: (ephemeral.len() + derived.len() + verified.len() + canonical.len()) as u64,
        })
    }
}

/// 稳定性统计
#[derive(Debug)]
pub struct StabilityStats {
    pub ephemeral_count: u64,
    pub derived_count: u64,
    pub verified_count: u64,
    pub canonical_count: u64,
    pub total_count: u64,
}
