# NDC 完整工程约束流程设计

> 整合知识库 + TODO 管理 + 任务分解 + 开发验收 + 工业级自治优化

---

## 1. 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          NDC 工业级自治系统                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    知识库 (Knowledge Base)                           │   │
│  │  - 项目文档 (README, ARCHITECTURE, API Docs)                       │   │
│  │  - 代码知识 (CodeKnowledge)                                          │   │
│  │  - 决策记录 (Decision Records)                                      │   │
│  │  - 变更历史 (Change History)                                        │   │
│  │  - 不变量约束 (Invariants) - Gold Memory                          │   │
│  │  - 波动热力图 (Volatility Heatmap) ← 新增                         │   │
│  │  - 稳定性层级: Ephemeral → Derived → Verified → Canonical         │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    TODO 管理 (Task Mapping)                          │   │
│  │  - 总 TODO (Project-Level)                                          │   │
│  │  - 子 TODO (Feature-Level)                                          │   │
│  │  - 任务分解链 (Task Chain)                                          │   │
│  │  - 状态追踪 (State Tracking)                                         │   │
│  │  - 约束容器 (ConstraintSet)                                        │   │
│  │  - 波动性分数 (VolatilityScore)                                    │   │
│  │  - Saga Undo Plan ← 新增                                           │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    任务执行 (Execution)                               │   │
│  │  - 步骤分解 (Step Decomposition)                                    │   │
│  │  - 开发执行 (Development)                                           │   │
│  │  - 影子探测 (Discovery Phase) ← 新增                               │   │
│  │  - 工作记忆 (Working Memory) ← 新增                                │   │
│  │  - 测试验证 (Testing)                                               │   │
│  │  - 质量门禁 (Quality Gate)                                        │   │
│  │  - 失败分类 (Failure Taxonomy) ← 新增                              │   │
│  │  - 回归测试强制 ← 新增 (Discovery→Hard Constraints)              │   │
│  │  - 验收确认 (Acceptance)                                           │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 完整流程状态机（事件驱动版）

```
                              ┌─────────────────────────────────────────┐
                              │           用户提出需求                    │
                              └─────────────────┬───────────────────────┘
                                                │
                                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 0: 任务谱系继承 (Lineage Inheritance)              │   │
│   │  - 检查父任务历史                                                   │   │
│   │  - 继承 Invariants (带版本标签)                                     │   │
│   │  - 继承 PostmortemContext                                          │   │
│   │  - 继承 Undo Plans                                                 │   │
│   │  - 输出: InheritedContext                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 1: 理解需求 (Understand)                         │   │
│   │  - 检索知识库 (含 Heatmap)                                         │   │
│   │  - 检索总 TODO                                                    │   │
│   │  - 谱系继承                                                       │   │
│   │  - Volatility 评估                                                 │   │
│   │  - 输出: RequirementContext                                        │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│                         总 TODO 映射存在?                                   │
│                         ┌──────────┴──────────┐                          │
│                         ▼                        ▼                          │
│               ┌──────────────────┐    ┌──────────────────────────┐      │
│               │ 是: 关联现有      │    │ 否: 建立新映射             │      │
│               │    TODO          │    │    (提示用户确认)          │      │
│               └────────┬─────────┘    └────────────┬─────────────┘      │
│                        │                          │                       │
│                        │                          ▼                       │
│                        │               ┌──────────────────────────┐      │
│                        │               │  阶段 2a: 建立 TODO 映射 │      │
│                        │               │  - 创建总 TODO 条目      │      │
│                        │               │  - 关联需求              │      │
│                        │               │  - 标记为 pending        │      │
│                        │               └────────────┬─────────────┘      │
│                        │                          │                       │
│                        │                          ▼                       │
│                        │               ┌──────────────────────────┐      │
│                        │               │  生成 Undo Plan (初始)   │ ← 新增 │
│                        │               └────────────┬─────────────┘      │
│                        │                          │                       │
│                        ▼                          │                       │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 2: 分解需求 (Decompose)                         │   │
│   │  - LLM 理解需求                                                   │   │
│   │  - 检索相关文档和知识                                               │   │
│   │  - 继承的上下文 (含版本化 Invariants)                              │   │
│   │  - 分解为原子子任务                                                 │   │
│   │  - 创建子 TODO 链                                                  │   │
│   │  - 记录任务依赖和顺序                                               │   │
│   │  - 分解Lint校验 ← 新增 (非LLM确定性校验)                          │   │
│   │  - 为每个子任务生成 Undo Plan ← 新增                              │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 3: 影子探测 (Discovery Phase) ← 重点修复        │   │
│   │  - Read-Only Impact Analysis                                        │   │
│   │  - 生成 ImpactReport                                               │   │
│   │  - 生成 Volatility Heatmap ← 新增                                  │   │
│   │  - **转化 Hard Constraints** ← 关键修复                          │   │
│   │    - High Volatility 模块 → 强制回归测试套件                        │   │
│   │    - 隐性耦合检测 (反射/宏/动态配置)                               │   │
│   │  - 评估 VolatilityRisk                                             │   │
│   │  - 决定是否需要升级验收                                             │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 4: 工作记忆快照 (Working Memory) ← 层次化设计   │   │
│   │  - 层次化结构: Abstract(History) + Raw(Current) + Hard(Invariants) ←新增│   │
│   │  - 包含: active_files, api_surface, recent_failures, invariants    │   │
│   │  - Failure Taxonomy 摘要 ← 新增                                  │   │
│   │  - 只在 SubTask 执行期存在                                          │   │
│   │  - 结束即销毁/归档                                                  │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 5: 执行开发 (Develop)                              │   │
│   │  ┌───────────────────────────────────────────────────────────┐   │   │
│   │  │ 子任务循环:                                                  │   │   │
│   │  │  5.1 开发执行 - 编写代码 (基于 Working Memory)               │   │   │
│   │  │  5.2 单元测试 - 编写/运行测试 (含回归测试)                  │   │   │
│   │  │  5.3 质量门禁 - cargo check/test/clippy                     │   │   │
│   │  │  5.4 验证结果 - 通过/失败                                   │   │   │
│   │  │  5.5 失败分类 - Failure Taxonomy ← 新增                   │   │   │
│   │  │  5.6 重来机制 - 最多 N 次                                   │   │   │
│   │  │          ↓ 失败                                               │   │   │
│   │  │  5.7 回滚执行 - 使用 Undo Plan ← 新增                       │   │   │
│   │  │          ↓ 超过阈值                                           │   │   │
│   │  │  5.8 人工介入 - Human Correction ← 新增                     │   │   │
│   │  └───────────────────────────────────────────────────────────┘   │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                           所有子任务完成?                                    │
│                           ┌──────────┴──────────┐                        │
│                           ▼                        ▼                        │
│                 ┌──────────────────┐    ┌──────────────────────────┐    │
│                 │ 是              │    │ 否: 继续执行              │    │
│                 └────────┬─────────┘    └────────────────────────┘     │
│                          │                                                │
│                          ▼                                                │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 6: 验收 (Accept)                                │   │
│   │  - 自动验收: 覆盖率 >= 80%, 所有测试通过                           │   │
│   │  - 人工验收: 需要复核的业务逻辑                                      │   │
│   │  - **强制回归测试** ← 新增 (Discovery 产出的 Hard Constraints)  │   │
│   │  - VolatilityRisk 触发加强版验收                                    │   │
│   │  - 输出: AcceptanceResult                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                          验收通过?                                          │
│                          ┌──────────┴──────────┐                        │
│                          ▼                        ▼                        │
│                ┌──────────────────┐    ┌──────────────────────────┐    │
│                │ 是              │    │ 否: 修复/重来              │    │
│                └────────┬─────────┘    └────────────────────────┘    │
│                         │                                                │
│                         ▼                                                │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 7: 失败归因与固化 (Failure → Invariant) ← 修复  │   │
│   │  - 失败分类: LogicError / TestGap / SpecAmbiguity / DecisionConflict│   │
│   │  - **Non-Deterministic Failure** ← 新增                          │   │
│   │    (相同条件下时好时坏 → 工具链/随机性问题)                         │   │
│   │  - Human Correction → Invariant (带 TTL/Version) ← 关键修复       │   │
│   │    - Invariant 增加: TTL, version_sensitivity                    │   │
│   │  - 生成 PostmortemContext                                         │   │
│   │  - 冲突检测 ← 新增 (Invariant 互斥检查)                           │   │
│   │  - 回灌到 KnowledgeBase                                           │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 8: 更新文档 (Document)                           │   │
│   │  - 更新相关文档 (Fact Docs + Narrative Docs)                     │   │
│   │  - 记录决策和变更                                                  │   │
│   │  - 提升知识库稳定性                                                │   │
│   │  - 更新 Undo Plan (基于执行经验) ← 新增                          │   │
│   │  - 输出: DocumentChanges                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 9: 完成 (Complete)                              │   │
│   │  - 标记总 TODO 完成                                               │   │
│   │  - 发送完成通知                                                   │   │
│   │  - **更新 Telemetry** ← 新增                                      │   │
│   │    - Autonomous Rate                                              │   │
│   │    - Correction Cost                                               │   │
│   │    - Token Efficiency                                             │   │
│   │  - 更新 TaskLineage (为后续继承)                                  │   │
│   │  - 输出: CompletionReport                                        │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 工业级优化组件设计 (修复版)

### 3.1 Discovery Phase → Hard Constraints (关键修复)

```rust
// crates/runtime/src/discovery/hard_constraints.rs

/// Discovery 产出的 Hard Constraints
/// 确保执行阶段不会忽略 Discovery 的发现

#[derive(Debug, Clone)]
pub struct HardConstraints {
    /// 强制回归测试套件
    pub mandatory_regression_tests: Vec<TestSpec>,

    /// 必须验证的 API 表面
    pub verified_api_surface: Vec<ApiSymbol>,

    /// 高波动模块列表
    pub high_volatility_modules: Vec<ModuleId>,

    /// 隐性耦合警告
    pub隐性_coupling_warnings: Vec<CouplingWarning>,

    /// 版本敏感约束
    pub version_sensitive_constraints: Vec<VersionedConstraint>,
}

impl HardConstraints {
    /// 注入到 QualityGate
    pub async fn inject_into_quality_gate(&self, gate: &mut QualityGate) {
        // 为每个高波动模块添加回归测试
        for module in &self.high_volatility_modules {
            gate.add_regression_test(RegressionTest {
                module: module.clone(),
                tests: self.mandatory_regression_tests.clone(),
            });
        }

        // 注入 API 表面验证
        gate.set_api_surface_validation(&self.verified_api_surface);
    }
}

/// 隐性耦合检测
#[derive(Debug, Clone)]
pub struct CouplingWarning {
    pub source: FileRef,
    pub target: FileRef,
    pub coupling_type: CouplingType,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone)]
pub enum CouplingType {
    Reflection,
    MacroInjection,
    DynamicConfig,
    RuntimePolymorphism,
}
```

### 3.2 Volatility Heatmap (新增)

```rust
// crates/runtime/src/discovery/heatmap.rs

/// 波动热力图 - 通过 git 历史计算模块变更频率

#[derive(Debug, Clone)]
pub struct VolatilityHeatmap {
    /// 模块变更频率
    module_frequency: HashMap<ModuleId, f64>,

    /// 最近变更文件 (N 天内)
    recent_changes: Vec<FileChange>,

    /// 核心模块列表 (高风险区域)
    core_modules: Vec<ModuleId>,
}

impl VolatilityHeatmap {
    /// 通过 git log 计算
    pub async fn from_git(&self, repo_path: &Path, days: u32) -> Self {
        let since = Utc::now() - chrono::Duration::days(days as i64);

        // 获取变更文件
        let changes = git_log_files(repo_path, since).await;

        // 计算频率
        let mut frequency = HashMap::new();
        for change in &changes {
            let module = self.identify_module(&change.path);
            *frequency.entry(module).or_insert(0.0) += 1.0;
        }

        // 归一化
        let max_freq = frequency.values().cloned().fold(0.0f64, f64::max);
        for module in frequency.keys() {
            frequency.insert(module.clone(), frequency[module] / max_freq);
        }

        Self {
            module_frequency: frequency,
            recent_changes: changes,
            core_modules: self.load_core_modules(),
        }
    }

    /// 标记无论 LLM 如何评估都视为高风险的文件
    pub fn mark_high_risk(&self, file: &Path) -> bool {
        // 检查是否在核心模块
        let module = self.identify_module(file);
        let freq = self.module_frequency.get(&module).unwrap_or(&0.0);

        // 热力图规则: 最近7天变更超过5次 = 高风险
        let recent_count = self.recent_changes
            .iter()
            .filter(|c| c.path.starts_with(&module.path))
            .count();

        recent_count > 5 || *freq > 0.8
    }
}
```

### 3.3 Working Memory (层次化修复)

```rust
// crates/core/src/memory/working_memory.rs

/// 工作记忆 - 层次化结构
/// 核心原则: Abstract(History) + Raw(Current) + Hard(Invariants)

#[derive(Debug, Clone)]
pub struct WorkingMemory {
    /// 作用域: 当前子任务
    pub scope: SubTaskId,

    /// 层级 1: 抽象历史 (Abstract)
    /// 记录为什么失败，而非仅仅失败了什么
    pub abstract_history: AbstractHistory,

    /// 层级 2: 原始当前 (Raw Current)
    /// 当前步骤的原始信息
    pub raw_current: RawCurrent,

    /// 层级 3: 硬约束 (Hard Invariants)
    /// 来自 Gold Memory 的不变量
    pub hard_invariants: Vec<VersionedInvariant>,
}

#[derive(Debug, Clone)]
pub struct AbstractHistory {
    /// 失败模式摘要 (Why it failed)
    pub failure_patterns: Vec<FailurePattern>,

    /// 根因分析摘要
    pub root_cause_summary: Option<String>,

    /// 尝试次数
    pub attempt_count: u8,

    /// 状态: cycling / progressing / stuck
    pub trajectory_state: TrajectoryState,
}

impl AbstractHistory {
    /// 检测是否陷入循环
    pub fn detect_cycle(&self) -> bool {
        // 如果多次失败且根因相同 = 循环
        self.attempt_count > 3 &&
        self.failure_patterns.iter()
            .filter(|p| p.same_root_cause())
            .count() > 2
    }
}

#[derive(Debug, Clone)]
pub enum TrajectoryState {
    Progressing { steps_since_last_failure: u8 },
    Cycling { repeated_pattern: String },
    Stuck { last_error: String },
}

impl WorkingMemory {
    /// 生成: 从 Discovery + History + Inherited Invariants
    pub fn generate(
        impact_report: &ImpactReport,
        abstract_history: &AbstractHistory,
        inherited_invariants: &[VersionedInvariant],
    ) -> Self {
        Self {
            scope: impact_report.task_id.clone(),
            abstract_history: abstract_history.clone(),
            raw_current: RawCurrent {
                active_files: impact_report.touched_files.clone(),
                api_surface: impact_report.public_api_changes.clone(),
                current_step_context: None,
            },
            hard_invariants: inherited_invariants.to_vec(),
        }
    }

    /// 提供给 LLM 的精简上下文
    pub fn精简_context_for_llm(&self) -> LlmContext {
        LlmContext {
            history: format!(
                " Attempts: {}, Status: {:?}, Last Error: {:?}",
                self.abstract_history.attempt_count,
                self.abstract_history.trajectory_state,
                self.abstract_history.root_cause_summary
            ),
            current_files: &self.raw_current.active_files,
            apis: &self.raw_current.api_surface,
            invariants: &self.hard_invariants
                .iter()
                .map(|i| format!("MUST: {}", i.rule))
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
}
```

### 3.4 Invariant with TTL & Version Sensitivity (关键修复)

```rust
// crates/core/src/memory/invariant.rs

/// 不变量约束 - Gold Memory with TTL/Version

#[derive(Debug, Clone)]
pub struct VersionedInvariant {
    pub invariant: Invariant,

    /// TTL: 过期时间 (None = 永不过期)
    pub ttl: Option<Duration>,

    /// 版本敏感标签
    pub version_tags: Vec<VersionTag>,

    /// 创建时间
    pub created_at: Timestamp,

    /// 最后验证时间
    pub last_validated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionTag {
    pub dimension: VersionDimension,
    pub value: String,
    pub operator: VersionOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionDimension {
    RustVersion,
    CrateVersion { name: String },
    Platform { target: String },
    Dependency { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionOperator {
    Exact,      // ==
    AtLeast,    // >=
    AtMost,     // <=
    Compatible, // ~=
}

impl VersionedInvariant {
    /// 检查是否适用于当前环境
    pub fn is_applicable(&self, current_env: &Environment) -> bool {
        // 1. 检查 TTL
        if let Some(ttl) = self.ttl {
            if self.created_at.elapsed() > ttl {
                return false; // 已过期
            }
        }

        // 2. 检查版本标签
        for tag in &self.version_tags {
            let current = current_env.get_version(&tag.dimension);
            if !tag.matches(current) {
                return false;
            }
        }

        true
    }

    /// 检查是否与现有 Invariant 冲突
    pub fn check_conflict(&self, others: &[VersionedInvariant]) -> Vec<Conflict> {
        let mut conflicts = Vec::new();

        for other in others {
            if self.scope == other.scope {
                // 检查规则是否互斥
                if self.rule.is_mutually_exclusive(&other.rule) {
                    conflicts.push(Conflict {
                        invariant_a: self.id,
                        invariant_b: other.id,
                        conflict_type: MutuallyExclusive,
                    });
                }
            }
        }

        conflicts
    }

    /// 从人类纠正生成
    pub fn from_human_correction(
        correction: &HumanCorrection,
        scope: InvariantScope,
    ) -> Self {
        Self {
            invariant: Invariant {
                id: InvariantId::new(),
                scope,
                rule: FormalConstraint::from自然_language(&correction.violated_assumption),
                source: correction.clone(),
                priority: InvariantPriority::Highest,
            },
            // 默认 TTL: 90天，可配置
            ttl: Some(Duration::from_secs(90 * 24 * 60 * 60)),
            version_tags: vec![
                VersionTag {
                    dimension: VersionDimension::RustVersion,
                    value: env!("RUST_VERSION").to_string(),
                    operator: VersionOperator::AtLeast,
                }
            ],
            created_at: Utc::now(),
            last_validated_at: None,
        }
    }
}
```

### 3.5 Saga Pattern & Undo Plan (新增)

```rust
// crates/runtime/src/execution/saga.rs

/// Saga Pattern - 任务回滚计划
/// 确保执行到一半失败时可以干净回滚

#[derive(Debug, Clone)]
pub struct SagaPlan {
    /// Saga ID
    pub id: SagaId,

    /// 根任务 ID
    pub root_task_id: TaskId,

    /// 步骤列表 (带 Undo 动作)
    pub steps: Vec<SagaStep>,

    /// Compensating Transaction 列表
    pub compensations: Vec<CompensationAction>,
}

#[derive(Debug, Clone)]
pub struct SagaStep {
    pub step_id: StepId,
    pub action: StepAction,
    pub undo_action: Option<UndoAction>,
    pub status: StepStatus,
}

#[derive(Debug, Clone)]
pub enum UndoAction {
    /// 删除文件
    DeleteFile { path: PathBuf },

    /// 恢复文件
    RestoreFile { path: PathBuf, backup: String },

    /// 执行 Shell 命令
    ShellCommand { command: String, args: Vec<String> },

    /// Git 回滚
    GitRevert { commit_hash: String },

    /// 自定义补偿
    Custom { handler: String, params: serde_json::Value },
}

impl SagaPlan {
    /// 从 TaskChain 生成 Undo Plan
    pub fn from_task_chain(task_chain: &TaskChain) -> Self {
        let mut steps = Vec::new();
        let mut compensations = Vec::new();

        for subtask in &task_chain.subtasks {
            // 为每个子任务生成 Undo 动作
            let undo = Self::generate_undo_for_subtask(subtask);

            steps.push(SagaStep {
                step_id: subtask.id.clone(),
                action: StepAction::from(&subtask.action),
                undo_action: undo.clone(),
                status: StepStatus::Pending,
            });

            if let Some(u) = undo {
                compensations.push(CompensationAction {
                    step_id: subtask.id.clone(),
                    undo: u,
                });
            }
        }

        Self {
            id: SagaId::new(),
            root_task_id: task_chain.root_task_id.clone(),
            steps,
            compensations,
        }
    }

    /// 执行回滚
    pub async fn rollback(&self, from_step: &StepId) -> Result<(), RollbackError> {
        // 找到步骤索引
        let start_idx = self.steps
            .iter()
            .position(|s| s.step_id == *from_step)
            .ok_or(RollbackError::StepNotFound)?;

        // 倒序执行补偿
        for step in self.steps[..=start_idx].iter().rev() {
            if let Some(ref undo) = step.undo_action {
                self.execute_undo(undo).await?;
                step.status = StepStatus::RolledBack;
            }
        }

        Ok(())
    }
}
```

### 3.6 Failure Taxonomy (新增 Non-Deterministic)

```rust
// crates/core/src/error/taxonomy.rs

/// 失败分类 - 带 Non-Deterministic 检测

#[derive(Debug, Clone, PartialEq)]
pub enum FailureTaxonomy {
    /// 逻辑错误
    LogicError {
        location: FileRef,
        expected: String,
        actual: String,
    },

    /// 测试缺口
    TestGap {
        missing_tests: Vec<TestCase>,
        coverage_delta: f64,
    },

    /// 规格歧义
    SpecAmbiguity {
        ambiguous_part: String,
        possible_interpretations: Vec<String>,
    },

    /// 决策冲突
    DecisionConflict {
        previous_decision: DecisionId,
        conflicting_change: ChangeId,
    },

    /// 工具失败
    ToolFailure {
        tool: String,
        error: String,
        is_transient: bool,
    },

    /// 人类纠正 ← 新增
    HumanCorrection {
        reason: String,
        violated_assumption: String,
        generated_invariant: VersionedInvariant,
    },

    /// **Non-Deterministic Failure** ← 新增
    /// 相同条件下时好时坏 = 工具链/随机性问题
    NonDeterministic {
        symptom: String,
        occurrence_count: u32,
        probability: f64,
        likely_cause: NonDeterministicCause,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum NonDeterministicCause {
    /// 并发竞争条件
    RaceCondition,

    /// 随机数/时间依赖
    TimeDependency,

    /// 网络/IO 不稳定
    NetworkInstability,

    /// LLM 随机性
    LlmNonDeterminism,

    /// 未分类
    Unknown,
}

impl FailureTaxonomy {
    /// 是否为 Non-Deterministic
    pub fn is_non_deterministic(&self) -> bool {
        matches!(self, FailureTaxonomy::NonDeterministic { .. })
    }

    /// 根据分类决定下一步
    pub fn should_retry(&self) -> bool {
        match self {
            FailureTaxonomy::LogicError => true,
            FailureTaxonomy::TestGap => true,
            FailureTaxonomy::SpecAmbiguity => false,
            FailureTaxonomy::DecisionConflict => false,
            FailureTaxonomy::ToolFailure { is_transient, .. } => *is_transient,
            FailureTaxonomy::HumanCorrection { .. } => false,
            FailureTaxonomy::NonDeterministic { .. } => {
                // Non-Deterministic: 不建议无限重试，需要诊断根本原因
                false
            }
        }
    }
}
```

### 3.7 Telemetry (新增)

```rust
// crates/core/src/telemetry/mod.rs

/// 遥测系统 - 衡量工业级自治能力

#[derive(Debug, Default)]
pub struct Telemetry {
    /// 任务统计
    task_stats: TaskStats,

    /// 人工介入统计
    human_intervention_stats: HumanStats,

    /// Token 效率
    token_efficiency: TokenEfficiency,

    /// 模型切换统计
    model_switch_stats: ModelSwitchStats,
}

#[derive(Debug, Default)]
pub struct TaskStats {
    total: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    avg_duration_ms: AtomicU64,
}

impl TaskStats {
    pub fn record_completed(&self, duration_ms: u64) {
        self.total.fetch_add(1, Ordering::SeqCst);
        self.completed.fetch_add(1, Ordering::SeqCst);
        // 更新平均耗时
        let _ = self.avg_duration_ms.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |_| Some(duration_ms));
    }
}

#[derive(Debug, Default)]
pub struct HumanStats {
    total_interventions: AtomicU64,
    total_time_ms: AtomicU64,
    invariant_generations: AtomicU64,
}

impl HumanStats {
    pub fn record_intervention(&self, duration_ms: u64) {
        self.total_interventions.fetch_add(1, Ordering::SeqCst);
        self.total_time_ms.fetch_add(duration_ms, Ordering::SeqCst);
    }

    /// 平均人工介入成本
    pub fn avg_intervention_cost(&self) -> f64 {
        let count = self.total_interventions.load(Ordering::SeqCst);
        let time = self.total_time_ms.load(Ordering::SeqCst);
        if count > 0 { time as f64 / count as f64 } else { 0.0 }
    }
}

impl Telemetry {
    /// Autonomous Rate = 无需人类干预完成任务的比例
    pub fn autonomous_rate(&self) -> f64 {
        let total = self.task_stats.total.load(Ordering::SeqCst);
        let completed = self.task_stats.completed.load(Ordering::SeqCst);
        let interventions = self.human_intervention_stats.total_interventions.load(Ordering::SeqCst);

        if total == 0 { 0.0 } else {
            // 扣除人工介入次数后的自主完成率
            (completed.saturating_sub(interventions) as f64) / total as f64
        }
    }

    /// Token Efficiency = 复杂度 / Token 消耗
    pub fn token_efficiency(&self) -> f64 {
        self.token_efficiency.complexity_per_token()
    }

    /// 报告
    pub fn report(&self) -> TelemetryReport {
        TelemetryReport {
            autonomous_rate: self.autonomous_rate(),
            avg_intervention_cost_ms: self.human_stats.avg_intervention_cost(),
            token_efficiency: self.token_efficiency(),
            model_switches: self.model_switch_stats.total(),
        }
    }
}
```

---

## 4. Model Selector (Cost-Aware 细化)

```rust
// crates/core/src/llm/selector.rs

/// 模型选择器 - Cost-Aware + Risk-Aware

pub struct ModelSelector {
    providers: HashMap<String, ProviderInfo>,
}

pub struct ProviderInfo {
    provider: Arc<dyn LlmProvider>,
    cost_per_token: f64,
    latency_ms: f64,
    capability_score: f64,
}

impl ModelSelector {
    pub fn select(&self, entropy: &TaskEntropy) -> Arc<dyn LlmProvider> {
        // 计算综合评分
        let mut candidates = Vec::new();

        for (name, info) in &self.providers {
            let risk_score = self.calculate_risk_score(entropy);
            let cost_score = self.calculate_cost_score(info.cost_per_token, entropy);
            let latency_score = self.calculate_latency_score(info.latency_ms);
            let capability_score = info.capability_score;

            // 综合评分: 能力优先，风险兜底，成本敏感
            let total_score = capability_score * 0.4
                + (1.0 - risk_score) * 0.3
                + (1.0 - cost_score) * 0.2
                + (1.0 - latency_score) * 0.1;

            candidates.push((name.clone(), total_score, info));
        }

        // 排序选择最高分
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // 高风险任务强制使用最强模型
        if self.is_high_risk(entropy) {
            return self.get_strongest_provider();
        }

        candidates.first().map(|(_, _, info)| info.provider.clone()).unwrap()
    }

    fn is_high_risk(&self, entropy: &TaskEntropy) -> bool {
        entropy.volatility.score() > 0.7 || entropy.has_invariants()
    }
}
```

---

## 5. 配置设计 (完整版)

```yaml
# NDC 工程配置 - 工业级自治版

engineering:
  # 阶段 0: 谱系继承
  lineage:
    enabled: true
    inherit_invariants: true
    inherit_failures: true
    inherit_undo_plans: true
    max_context_depth: 3

  # 阶段 1: 需求理解
  understanding:
    timeout: 60
    similarity_threshold: 0.8
    use_heatmap: true

  # 阶段 2: 任务分解
  decomposition:
    max_subtasks: 20
    enable_lint: true
    enable_undo_plan: true
    lint_rules:
      - no_cyclic_dependency
      - all_verifiable
      - no_overly_broad

  # 阶段 3: 影子探测
  discovery:
    enabled: true
    required_for_high_volatility: true
    risk_threshold: 0.7
    generate_hard_constraints: true
    heatmap_days: 7
    heatmap_frequency_threshold: 5

  # 阶段 4: 工作记忆
  working_memory:
    enabled: true
    max_recent_failures: 3
    auto_archive: true
    enable_abstraction: true

  # 阶段 5: 执行
  execution:
    max_retries: 3
    enable_saga_rollback: true
    quality_gates:
      - "cargo check"
      - "cargo test --lib"
      - "cargo clippy"
    regression_tests_for_high_volatility: true

  # 阶段 6: 验收
  acceptance:
    require_human: false
    enhanced_for_high_risk: true
    coverage_threshold: 0.8

  # 阶段 7: 失败归因
  failure_handling:
    enable_taxonomy: true
    human_to_invariant: true
    invariant_ttl_days: 90
    conflict_detection: true

  # 阶段 9: 完成
  telemetry:
    enabled: true
    track_autonomous_rate: true
    track_intervention_cost: true
    track_token_efficiency: true

# LLM 配置 - Cost-Aware
llm:
  providers:
    fast:
      type: "gemini-flash"
      cost_per_token: 0.0001
      latency_ms: 100
      capability_score: 0.6
    balanced:
      type: "gpt-4o-mini"
      cost_per_token: 0.0005
      latency_ms: 200
      capability_score: 0.8
    strong:
      type: "gpt-4o"
      cost_per_token: 0.005
      latency_ms: 500
      capability_score: 0.95

  selector:
    default_strategy: "cost-risk-balanced"
    force_strong_for_high_risk: true
```

---

## 6. 状态流转总结

| 阶段 | 名称 | 关键修复/新增 | Telemetry |
|------|------|---------------|-----------|
| 0 | 谱系继承 | 继承版本化 Invariants + Undo Plans | - |
| 1 | 理解需求 | Volatility Heatmap | - |
| 2 | 分解需求 | Undo Plan 生成 | - |
| 3 | 影子探测 | **Hard Constraints** ⭐ | - |
| 4 | 工作记忆 | **层次化**: Abstract+Raw+Hard | - |
| 5 | 执行 | Saga Rollback | - |
| 6 | 验收 | 强制回归测试 | - |
| 7 | 失败归因 | **TTL/Version** + **Non-Deterministic** ⭐ | Invariant Gen |
| 8 | 更新文档 | Undo Plan 更新 | - |
| 9 | 完成 | **Telemetry** ⭐ | Autonomous Rate |

---

## 7. 实施优先级

### ⭐ 第一刀：Discovery Phase (完整版)

```
包含:
- ImpactReport → Hard Constraints
- Volatility Heatmap (git 历史)
- 隐性耦合检测

验收标准:
- [x] HardConstraints 结构 (hard_constraints.rs)
- [x] Heatmap 计算 (heatmap.rs)
- [x] 强制回归测试注入 (hard_constraints.rs:RegressionTest)
- [x] 隐性耦合检测 (hard_constraints.rs:CouplingWarning)
- [x] 触发加强验收逻辑 (mod.rs:should_generate_constraints)

**测试覆盖**: 15/15 通过
**提交**: ec499ab

---

### ⭐ 第二刀：Working Memory (层次化) + Saga Undo ✅ 已完成

```
职责: 执行态认知边界 + 任务回滚

层级结构:
- Abstract(History): 失败模式摘要 + 根因分析
- Raw(Current): 当前步骤文件 + API 表面
- Hard(Invariants): Gold Memory 不变量

测试覆盖: 5/5 通过
Saga Pattern: 7/7 通过

实现文件:
- crates/core/src/memory/working_memory.rs
- crates/runtime/src/execution/mod.rs
```

### ⭐ 第三刀：Invariant (TTL/Version) + Telemetry

---

## 8. 核心优势总结

| 维度 | 优化前 | 优化后 (修复版) |
|------|-------|----------------|
| **Discovery** | 报告脱节 | Hard Constraints 注入 |
| **Gold Memory** | 过时/冲突 | TTL + Version + 冲突检测 |
| **Working Memory** | 循环焦虑 | 层次化 + Cycle Detection |
| **执行回滚** | 缺失 | Saga Pattern |
| **失败分类** | 基础 | **Non-Deterministic** |
| **指标** | 缺失 | **Telemetry** (Autonomous Rate) |
| **模型调度** | 基础 | **Cost-Aware** |
| **风险控制** | 静态 | **Heatmap** 动态 |

---

> **一句话总结**: NDC 已从"工程控制系统"进化到"工业级自治系统"——具备认知闭环、版本感知、成本敏感、自动进化的完整能力。
