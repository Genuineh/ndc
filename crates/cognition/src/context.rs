//! Context Builder - 上下文组装模块
//!
//! 职责：
//! - 根据任务需求组装上下文
//! - 三层记忆检索（L1/L2/L3）
//! - 上下文压缩与裁剪
//! - Token 预算管理

use ndc_core::{Memory, MemoryStability, AgentRole};
use crate::{MemoryStore, VectorSearch, StabilityManager};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info};

/// 上下文构建错误
#[derive(Debug, Error)]
pub enum ContextBuilderError {
    #[error("记忆检索失败: {0}")]
    RetrievalFailed(String),

    #[error("Token 超出预算: 需要 {required}, 最大 {max}")]
    TokenBudgetExceeded { required: usize, max: usize },

    #[error("上下文组装失败: {0}")]
    AssemblyFailed(String),
}

/// 上下文配置
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// 最大 token 数
    pub max_tokens: usize,

    /// 优先级权重
    pub stability_weights: StabilityWeights,

    /// 是否启用压缩
    pub enable_compression: bool,

    /// 最大记忆数量
    pub max_memories: u32,

    /// 是否包含 L1（临时记忆）
    pub include_ephemeral: bool,

    /// 是否包含 L2（语义网络）
    pub include_derived: bool,

    /// 是否包含 L3（规范库）
    pub include_canonical: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            stability_weights: StabilityWeights::default(),
            enable_compression: true,
            max_memories: 50,
            include_ephemeral: true,
            include_derived: true,
            include_canonical: true,
        }
    }
}

/// 稳定性权重（用于排序）
#[derive(Debug, Clone)]
pub struct StabilityWeights {
    pub ephemeral: f64,
    pub derived: f64,
    pub verified: f64,
    pub canonical: f64,
}

impl Default for StabilityWeights {
    fn default() -> Self {
        Self {
            ephemeral: 0.1,   // 低优先级
            derived: 0.5,     // 中优先级
            verified: 0.8,    // 高优先级
            canonical: 1.0,   // 最高优先级
        }
    }
}

/// 构建的上下文
#[derive(Debug, Clone)]
pub struct BuiltContext {
    /// 检索到的记忆
    pub memories: Vec<Memory>,

    /// 上下文字符串
    pub context_string: String,

    /// Token 数量
    pub token_count: usize,

    /// 使用的记忆层级
    pub used_layers: Vec<MemoryStability>,
}

/// 上下文构建器
#[derive(Debug)]
pub struct ContextBuilder {
    /// 记忆存储
    memory_store: Arc<MemoryStore>,

    /// 向量搜索
    vector_search: Arc<VectorSearch>,

    /// 稳定性管理器
    stability_manager: Arc<StabilityManager>,

    /// 配置
    config: ContextConfig,
}

impl ContextBuilder {
    /// 创建新的上下文构建器
    pub fn new(
        memory_store: Arc<MemoryStore>,
        vector_search: Arc<VectorSearch>,
        stability_manager: Arc<StabilityManager>,
        config: ContextConfig,
    ) -> Self {
        Self {
            memory_store,
            vector_search,
            stability_manager,
            config,
        }
    }

    /// 创建默认配置的构建器
    pub fn default(
        memory_store: Arc<MemoryStore>,
        vector_search: Arc<VectorSearch>,
    ) -> Self {
        Self::new(
            memory_store,
            vector_search,
            Arc::new(StabilityManager::default(memory_store.clone())),
            ContextConfig::default(),
        )
    }

    /// 根据查询构建上下文
    pub async fn build_context(
        &self,
        query: &str,
        role: AgentRole,
    ) -> Result<BuiltContext, ContextBuilderError> {
        debug!("Building context for query: {}", query);

        // 1. 三层检索
        let mut memories = self.retrieve_three_layers(query).await
            .map_err(|e| ContextBuilderError::RetrievalFailed(e.to_string()))?;

        // 2. 按优先级排序
        self.sort_by_priority(&mut memories);

        // 3. 应用角色过滤
        self.filter_by_role(&mut memories, role);

        // 4. Token 预算控制
        let (memories, token_count) = self.apply_token_budget(memories).await
            .map_err(|e| ContextBuilderError::TokenBudgetExceeded {
                required: e.required,
                max: e.max,
            })?;

        // 5. 构建上下文字符串
        let context_string = self.assemble_context(&memories, query).await
            .map_err(|e| ContextBuilderError::AssemblyFailed(e.to_string()))?;

        // 6. 压缩（如启用）
        let context_string = if self.config.enable_compression {
            self.compress_context(&context_string)
        } else {
            context_string
        };

        // 获取使用的层级
        let used_layers: Vec<_> = memories.iter()
            .map(|m| m.content.stability)
            .collect();

        Ok(BuiltContext {
            memories,
            context_string,
            token_count,
            used_layers,
        })
    }

    /// 三层检索
    async fn retrieve_three_layers(&self, query: &str) -> Result<Vec<Memory>, String> {
        let mut all_memories = Vec::new();

        // L1: 工作空间（临时记忆）
        if self.config.include_ephemeral {
            let l1 = self.memory_store.get_by_stability(
                MemoryStability::Ephemeral,
            ).await
                .map_err(|e| e.to_string())?;

            // 语义搜索过滤
            let l1_filtered = self.filter_by_semantic(&l1, query).await;
            all_memories.extend(l1_filtered);
        }

        // L2: 语义网络
        if self.config.include_derived {
            let l2 = self.memory_store.get_by_stability(
                MemoryStability::Derived,
            ).await
                .map_err(|e| e.to_string())?;

            // 语义搜索过滤
            let l2_filtered = self.filter_by_semantic(&l2, query).await;
            all_memories.extend(l2_filtered);
        }

        // L3: 规范库（最高优先级，强制包含）
        if self.config.include_canonical {
            let l3 = self.memory_store.get_by_stability(
                MemoryStability::Canonical,
            ).await
                .map_err(|e| e.to_string())?;

            // L3 全量包含（作为"法律"）
            all_memories.extend(l3);
        }

        // 限制数量
        all_memories.truncate(self.config.max_memories as usize);

        debug!("Retrieved {} memories from all layers", all_memories.len());

        Ok(all_memories)
    }

    /// 语义过滤
    async fn filter_by_semantic<'a>(
        &self,
        memories: &'a [Memory],
        query: &str,
    ) -> Vec<Memory> {
        if memories.is_empty() {
            return vec![];
        }

        // 使用向量搜索
        let results = self.vector_search.search(query, 20).await
            .unwrap_or_default();

        let matched_ids: std::collections::HashSet<_> = results.iter()
            .map(|r| r.memory_id)
            .collect();

        memories.iter()
            .filter(|m| matched_ids.contains(&m.id))
            .cloned()
            .collect()
    }

    /// 按优先级排序
    fn sort_by_priority(&self, memories: &mut Vec<Memory>) {
        let weights = &self.config.stability_weights;

        memories.sort_by(|a, b| {
            let weight_a = match a.content.stability {
                MemoryStability::Ephemeral => weights.ephemeral,
                MemoryStability::Derived => weights.derived,
                MemoryStability::Verified => weights.verified,
                MemoryStability::Canonical => weights.canonical,
            };

            let weight_b = match b.content.stability {
                MemoryStability::Ephemeral => weights.ephemeral,
                MemoryStability::Derived => weights.derived,
                MemoryStability::Verified => weights.verified,
                MemoryStability::Canonical => weights.canonical,
            };

            // 按权重降序，相同则按访问次数
            weight_b.partial_cmp(&weight_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.content.access_count.cmp(&a.content.access_count)
                })
        });
    }

    /// 按角色过滤
    fn filter_by_role(&self, memories: &mut Vec<Memory>, role: AgentRole) {
        // TODO: 实现角色访问控制
        // 目前所有角色都可以访问所有记忆
    }

    /// 应用 Token 预算
    async fn apply_token_budget(
        &self,
        mut memories: Vec<Memory>,
    ) -> Result<(Vec<Memory>, usize), (Vec<Memory>, usize, usize)> {
        let mut total_tokens = 0;
        let mut included = Vec::new();

        for memory in memories {
            let memory_tokens = self.estimate_tokens(&memory.content.body);

            if total_tokens + memory_tokens <= self.config.max_tokens {
                included.push(memory);
                total_tokens += memory_tokens;
            } else {
                break;
            }
        }

        if total_tokens > self.config.max_tokens {
            Err((included, total_tokens, self.config.max_tokens))
        } else {
            Ok((included, total_tokens))
        }
    }

    /// 估算 Token 数量
    fn estimate_tokens(&self, text: &str) -> usize {
        // 简单估算：平均 4 字符/token
        (text.len() / 4).max(1)
    }

    /// 组装上下文字符串
    async fn assemble_context(
        &self,
        memories: &[Memory],
        query: &str,
    ) -> Result<String, String> {
        let mut context = String::new();

        // 添加查询
        context.push_str(&format!("# Query\n{}\n\n", query));

        // 添加记忆（按层级分组）
        let mut grouped: std::collections::HashMap<_, Vec<_>> = std::collections::HashMap::new();
        for memory in memories {
            grouped.entry(memory.content.stability).or_default().push(memory);
        }

        // 按优先级输出
        for stability in [
            MemoryStability::Canonical,
            MemoryStability::Verified,
            MemoryStability::Derived,
            MemoryStability::Ephemeral,
        ] {
            if let Some(mems) = grouped.get(&stability) {
                context.push_str(&format!("\n## {:?} ({})\n", stability, mems.len()));
                context.push_str("```\n");

                for memory in mems {
                    context.push_str(&format!("[{}] ", memory.id);
                    context.push_str(&memory.content.body[..std::cmp::min(200, memory.content.body.len())]);
                    if memory.content.body.len() > 200 {
                        context.push_str("...");
                    }
                    context.push('\n');
                }

                context.push_str("```\n");
            }
        }

        Ok(context)
    }

    /// 压缩上下文
    fn compress_context(&self, context: &str) -> String {
        // TODO: 实现更复杂的压缩算法
        // 目前只移除多余的空行
        context.lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 上下文预算追踪器
#[derive(Debug)]
pub struct TokenBudget {
    /// 已用 Token
    used: usize,

    /// 最大 Token
    max: usize,

    /// 记忆预算
    memory_budget: usize,
}

impl TokenBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            used: 0,
            max: max_tokens,
            memory_budget: max_tokens / 3,  // 1/3 用于记忆
        }
    }

    pub fn remaining(&self) -> usize {
        self.max.saturating_sub(self.used)
    }

    pub fn can_add(&self, tokens: usize) -> bool {
        self.used + tokens <= self.max
    }

    pub fn add(&mut self, tokens: usize) {
        self.used = self.used.saturating_add(tokens);
    }
}
