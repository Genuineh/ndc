# NDC 完整工程约束流程设计

> 整合知识库 + TODO 管理 + 任务分解 + 开发验收 + 工业级优化

---

## 1. 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          NDC 知识驱动开发流程                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    知识库 (Knowledge Base)                           │   │
│  │  - 项目文档 (README, ARCHITECTURE, API Docs)                       │   │
│  │  - 代码知识 (CodeKnowledge)                                          │   │
│  │  - 决策记录 (Decision Records)                                      │   │
│  │  - 变更历史 (Change History)                                        │   │
│  │  - 不变量约束 (Invariants) - Gold Memory                           │   │
│  │  - 稳定性层级: Ephemeral → Derived → Verified → Canonical        │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                      │
│                                    ▼                                      │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    TODO 管理 (Task Mapping)                          │   │
│  │  - 总 TODO (Project-Level)                                          │   │
│  │  - 子 TODO (Feature-Level)                                          │   │
│  │  - 任务分解链 (Task Chain)                                          │   │
│  │  - 状态追踪 (State Tracking)                                         │   │
│  │  - 约束容器 (ConstraintSet)                                        │   │
│  │  - 波动性分数 (VolatilityScore)                                     │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                      │
│                                    ▼                                      │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    任务执行 (Execution)                            │   │
│  │  - 步骤分解 (Step Decomposition)                                   │   │
│  │  - 开发执行 (Development)                                          │   │
│  │  - 影子探测 (Discovery Phase) ← 新增                              │   │
│  │  - 工作记忆 (Working Memory) ← 新增                               │   │
│  │  - 测试验证 (Testing)                                               │   │
│  │  - 质量门禁 (Quality Gate)                                         │   │
│  │  - 失败分类 (Failure Taxonomy) ← 新增                             │   │
│  │  - 验收确认 (Acceptance)                                            │   │
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
│   │              阶段 0: 任务谱系继承 (Lineage Inheritance) ← 新增     │   │
│   │  - 检查父任务历史                                                    │   │
│   │  - 继承 Invariants                                                   │   │
│   │  - 继承 PostmortemContext                                           │   │
│   │  - 输出: InheritedContext                                           │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 1: 理解需求 (Understand)                         │   │
│   │  - 检索知识库                                                        │   │
│   │  - 检索总 TODO                                                       │   │
│   │  - 谱系继承                                                          │   │
│   │  - 输出: RequirementContext                                         │   │
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
│                        │               │  阶段 2a: 建立 TODO 映射  │      │
│                        │               │  - 创建总 TODO 条目       │      │
│                        │               │  - 关联需求               │      │
│                        │               │  - 标记为 pending         │      │
│                        │               └────────────┬─────────────┘      │
│                        │                          │                       │
│                        ▼                          │                       │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 2: 分解需求 (Decompose)                         │   │
│   │  - LLM 理解需求                                                    │   │
│   │  - 检索相关文档和知识                                               │   │
│   │  - 继承的上下文                                                     │   │
│   │  - 分解为原子子任务                                                 │   │
│   │  - 创建子 TODO 链                                                   │   │
│   │  - 记录任务依赖和顺序                                               │   │
│   │  - 分解Lint校验 ← 新增 (非LLM确定性校验)                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 3: 影子探测 (Discovery Phase) ← 新增           │   │
│   │  - Read-Only Impact Analysis                                       │   │
│   │  - 生成 ImpactReport                                               │   │
│   │  - 评估 VolatilityRisk                                             │   │
│   │  - 决定是否需要升级验收                                             │   │
│   │  触发条件: High Volatility 模块                                     │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 4: 工作记忆快照 (Working Memory) ← 新增        │   │
│   │  - 生成 ContextSummary                                             │   │
│   │  - 包含: active_files, api_surface, recent_failures, invariants  │   │
│   │  - 只在 SubTask 执行期存在                                          │   │
│   │  - 结束即销毁/归档                                                  │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 5: 执行开发 (Develop)                           │   │
│   │  ┌───────────────────────────────────────────────────────────┐   │   │
│   │  │ 子任务循环:                                                  │   │   │
│   │  │  5.1 开发执行 - 编写代码 (基于 Working Memory)               │   │   │
│   │  │  5.2 单元测试 - 编写/运行测试                                │   │   │
│   │  │  5.3 质量门禁 - cargo check/test/clippy                     │   │   │
│   │  │  5.4 验证结果 - 通过/失败                                    │   │   │
│   │  │  5.5 失败分类 - Failure Taxonomy ← 新增                   │   │   │
│   │  │          ↓ 失败                                               │   │   │
│   │  │  5.6 重来机制 - 最多 N 次                                    │   │   │
│   │  │          ↓ 超过阈值                                           │   │   │
│   │  │  5.7 人工介入 - Human Correction ← 新增                   │   │   │
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
│   │              阶段 6: 验收 (Accept)                               │   │
│   │  - 自动验收: 覆盖率 >= 80%, 所有测试通过                           │   │
│   │  - 人工验收: 需要复核的业务逻辑                                     │   │
│   │  - VolatilityRisk 触发加强版验收 ← 新增                          │   │
│   │  - 输出: AcceptanceResult                                        │   │
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
│   │              阶段 7: 失败归因与固化 (Failure → Invariant) ← 新增 │   │
│   │  - 失败分类: LogicError / TestGap / SpecAmbiguity / DecisionConflict │   │
│   │  - Human Correction → Invariant ← 新增 (Gold Memory)            │   │
│   │  - 生成 PostmortemContext ← 新增                                 │   │
│   │  - 回灌到 KnowledgeBase                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 8: 更新文档 (Document)                          │   │
│   │  - 更新相关文档 (Fact Docs + Narrative Docs) ← 新增              │   │
│   │  - 记录决策和变更                                                  │   │
│   │  - 提升知识库稳定性                                                 │   │
│   │  - 输出: DocumentChanges                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 9: 完成 (Complete)                              │   │
│   │  - 标记总 TODO 完成                                                │   │
│   │  - 发送完成通知                                                    │   │
│   │  - 更新 TaskLineage (为后续继承) ← 新增                           │   │
│   │  - 输出: CompletionReport                                        │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 工业级优化组件设计

### 3.1 Working Memory (工作记忆)

```rust
// crates/core/src/memory/working_memory.rs

/// 工作记忆 - 执行态认知边界
/// 特点: 强生命周期、非检索型、工程优先

#[derive(Debug, Clone)]
pub struct WorkingMemory {
    /// 作用域: 当前子任务
    pub scope: SubTaskId,

    /// 活跃文件
    pub active_files: Vec<FileRef>,

    /// API 表面
    pub api_surface: Vec<ApiSymbol>,

    /// 最近失败摘要 (最近 3 次)
    pub recent_failures: Vec<FailureSummary>,

    /// TODO 约束
    pub todo_constraints: ConstraintSet,

    /// 不变量引用 (Gold Memory)
    pub invariants: Vec<InvariantRef>,

    /// 波动性提示
    pub volatility_hint: VolatilityScore,
}

impl WorkingMemory {
    /// 生成时机: Discovery Phase 完成之后
    pub fn from_discovery(
        impact_report: &ImpactReport,
        failure_history: &[FailureSummary],
        inherited_invariants: &[InvariantRef],
    ) -> Self {
        Self {
            scope: impact_report.task_id.clone(),
            active_files: impact_report.touched_files.clone(),
            api_surface: impact_report.public_api_changes.clone(),
            recent_failures: failure_history.iter().take(3).cloned().collect(),
            todo_constraints: ConstraintSet::current(),
            invariants: inherited_invariants.to_vec(),
            volatility_hint: impact_report.volatility_risk.score(),
        }
    }

    /// 生命周期: SubTask 结束即销毁
    pub fn archive(&self) -> ArchivedWorkingMemory {
        ArchivedWorkingMemory {
            task_id: self.scope.clone(),
            compressed_context: self.compress(),
            archived_at: Utc::now(),
        }
    }

    fn compress(&self) -> CompressedContext {
        // 极简压缩: 只保留摘要
        CompressedContext {
            file_count: self.active_files.len(),
            api_count: self.api_surface.len(),
            failure_pattern: self.recent_failures.first().map(|f| f.pattern()),
            invariant_count: self.invariants.len(),
        }
    }
}
```

### 3.2 Discovery Phase (影子探测)

```rust
// crates/runtime/src/discovery/mod.rs

/// 影子探测阶段 - Read-Only 影响分析
/// 职责: 在动手术前先照 X 光

#[derive(Debug)]
pub struct DiscoveryPhase {
    fs_tool: FsTool,
    git_tool: GitTool,
    ast_scanner: AstScanner,
}

impl DiscoveryPhase {
    /// 执行探测
    pub async fn execute(
        &self,
        task: &TaskPlan,
    ) -> Result<ImpactReport, DiscoveryError> {
        // 1. 只读扫描
        let file_changes = self.scan_files(task).await?;
        let api_surface = self.scan_apis(task).await?;
        let git_impact = self.analyze_git_impact(task).await?;

        // 2. 生成影响报告
        let report = ImpactReport {
            touched_files: file_changes,
            affected_modules: self.identify_modules(&file_changes),
            public_api_changes: api_surface,
            test_impact: self.assess_test_impact(&file_changes),
            volatility_risk: self.calculate_volatility_risk(&file_changes),
            git_impact,
        };

        // 3. 如果高风险，自动触发加强版验收
        if report.volatility_risk.is_high() {
            self.trigger_enhanced_acceptance(&report)?;
        }

        Ok(report)
    }

    fn calculate_volatility_risk(&self, files: &[FileRef]) -> VolatilityScore {
        // 评估修改对核心模块的影响
        let core_modules = ["core/", "auth/", "payment/"];
        let touched_core = files.iter().any(|f| {
            core_modules.iter().any(|m| f.path.starts_with(m))
        });

        VolatilityScore {
            base: if touched_core { 0.8 } else { 0.3 },
            file_count_factor: (files.len() as f64 / 10.0).min(0.3),
            test_coverage_factor: if self.has_test_coverage(files) { -0.2 } else { 0.1 },
        }
    }
}

/// 影响报告
#[derive(Debug, Clone)]
pub struct ImpactReport {
    pub touched_files: Vec<FileRef>,
    pub affected_modules: Vec<ModuleId>,
    pub public_api_changes: Vec<ApiSymbol>,
    pub test_impact: TestSurface,
    pub volatility_risk: VolatilityScore,
    pub git_impact: GitImpact,
}

#[derive(Debug, Clone)]
pub struct VolatilityScore {
    pub base: f64,
    pub file_count_factor: f64,
    pub test_coverage_factor: f64,
}

impl VolatilityScore {
    pub fn score(&self) -> f64 {
        (self.base + self.file_count_factor + self.test_coverage_factor)
            .clamp(0.0, 1.0)
    }

    pub fn is_high(&self) -> bool {
        self.score() > 0.7
    }
}
```

### 3.3 Failure Taxonomy (失败分类)

```rust
// crates/core/src/error/taxonomy.rs

/// 失败分类 - 理解失败，而非盲目重试

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
        generated_invariant: Invariant,
    },
}

impl FailureTaxonomy {
    /// 根据分类决定下一步
    pub fn should_retry(&self) -> bool {
        match self {
            FailureTaxonomy::LogicError => true,
            FailureTaxonomy::TestGap => true,
            FailureTaxonomy::SpecAmbiguity => false,  // 需要回阶段1
            FailureTaxonomy::DecisionConflict => false, // 需要回阶段2
            FailureTaxonomy::ToolFailure { is_transient, .. } => *is_transient,
            FailureTaxonomy::HumanCorrection { .. } => false, // 产生 Invariant
        }
    }

    /// 返回哪个阶段
    pub fn fallback_stage(&self) -> WorkflowStage {
        match self {
            FailureTaxonomy::LogicError => WorkflowStage::Execution,
            FailureTaxonomy::TestGap => WorkflowStage::Execution,
            FailureTaxonomy::SpecAmbiguity => WorkflowStage::Understanding,
            FailureTaxonomy::DecisionConflict => WorkflowStage::Mapping,
            FailureTaxonomy::ToolFailure => WorkflowStage::CurrentStep,
            FailureTaxonomy::HumanCorrection { .. } => WorkflowStage::固化,
        }
    }
}
```

### 3.4 Human → Invariant → Gold Memory

```rust
// crates/core/src/memory/invariant.rs

/// 不变量约束 - 人类纠正固化为系统记忆
/// 价值: "同一个坑填过一次，永远不会再掉进去"

#[derive(Debug, Clone)]
pub struct Invariant {
    pub id: InvariantId,
    pub scope: InvariantScope,
    pub rule: FormalConstraint,
    pub source: HumanCorrection,
    pub priority: InvariantPriority,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone)]
pub enum InvariantScope {
    Module(ModuleId),
    Api(ApiSymbol),
    Project,
}

#[derive(Debug, Clone)]
pub enum InvariantPriority {
    Highest,    // 人类纠正产生
    High,       // 系统推理
    Medium,     // LLM 建议
}

impl Invariant {
    /// 从人类纠正生成
    pub fn from_human_correction(
        correction: &HumanCorrection,
        scope: InvariantScope,
    ) -> Self {
        Self {
            id: InvariantId::new(),
            scope,
            rule: FormalConstraint::from自然_language(&correction.violated_assumption),
            source: correction.clone(),
            priority: InvariantPriority::Highest,
            created_at: Utc::now(),
        }
    }

    /// 注入到系统
    pub async fn inject(&self, system: &mut System) {
        // 1. 加入 Gold Memory
        system.knowledge_base.add_gold_memory(self).await;

        // 2. 挂载到未来 WorkingMemory
        system.working_memory_tracker.register_invariant(self);

        // 3. 影响 Decomposition Validator
        system.decomposition_validator.add_invariant_constraint(self);

        // 4. 影响 ModelSelector (高风险提示)
        system.model_selector.mark_high_risk(self.scope.clone());
    }
}
```

### 3.5 模型自适应调度

```rust
// crates/core/src/llm/selector.rs

/// 模型选择器 - 根据任务熵动态调度

#[derive(Debug)]
pub struct TaskEntropy {
    /// 依赖深度
    pub dependency_depth: u8,

    /// 跨模块
    pub cross_module: bool,

    /// 波动性
    pub volatility: VolatilityScore,

    /// 不变量密度
    pub invariant_density: u8,
}

impl TaskEntropy {
    pub fn from_task(task: &TaskPlan, context: &ExecutionContext) -> Self {
        Self {
            dependency_depth: task.dependencies.len() as u8,
            cross_module: task.involves_multiple_modules(),
            volatility: context.volatility_hint.clone(),
            invariant_density: context.invariants.len() as u8,
        }
    }
}

pub struct ModelSelector {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
}

impl ModelSelector {
    pub fn select(&self, entropy: &TaskEntropy) -> Arc<dyn LlmProvider> {
        // 风险-熵函数
        match entropy {
            // 低风险: 快速便宜模型
            e if e.volatility.score() < 0.3 && e.invariant_density > 2 => {
                self.providers.get("fast").unwrap().clone()
            }

            // 中等风险: 均衡模型
            e if e.volatility.score() < 0.6 && !e.cross_module => {
                self.providers.get("balanced").unwrap().clone()
            }

            // 高风险 / 跨模块 / 违反不变量: 最强模型
            _ => self.providers.get("strong").unwrap().clone(),
        }
    }
}
```

### 3.6 任务谱系继承

```rust
// crates/core/src/todo/lineage.rs

/// 任务谱系 - 工程"老兵效应"

#[derive(Debug, Clone)]
pub struct TaskLineage {
    /// 父任务 ID
    pub parent: Option<TaskId>,

    /// 继承的不变量
    pub inherited_invariants: Vec<InvariantRef>,

    /// 继承的失败模式
    pub inherited_failures: Vec<FailurePattern>,

    /// 继承的上下文
    pub inherited_context: Option<ArchivedWorkingMemory>,
}

impl TaskLineage {
    /// 检查是否是新任务的后继
    pub async fn check_successor(
        &self,
        new_task: &TaskPlan,
        kb: &KnowledgeBase,
    ) -> Option<TaskLineage> {
        // 1. 查找父任务
        let parent = kb.find_related_completed_task(new_task).await?;

        // 2. 继承不变量
        let invariants = kb.get_invariants_for(parent.id).await;

        // 3. 继承失败模式
        let failures = kb.get_failure_patterns_for(parent.id).await;

        // 4. 继承工作记忆
        let context = kb.get_archived_context(parent.id).await;

        Some(TaskLineage {
            parent: Some(parent.id),
            inherited_invariants: invariants,
            inherited_failures: failures,
            inherited_context: context,
        })
    }
}
```

---

## 4. 事件驱动 Workflow Engine

```rust
// crates/runtime/src/engine/workflow_engine.rs

/// 事件驱动的 Workflow Engine
/// 特点: 不是线性阶段，而是状态+触发器

pub struct NdcWorkflowEngine {
    state_machine: WorkflowStateMachine,
    trigger_handler: TriggerHandler,
    artifact_registry: ArtifactRegistry,
}

impl NdcWorkflowEngine {
    /// 处理工作流事件
    pub async fn handle_event(&mut self, event: WorkflowEvent) -> Result<(), Error> {
        match event {
            WorkflowEvent::TaskSubmitted(input) => {
                self.on_task_submitted(input).await
            }
            WorkflowEvent::StageCompleted(stage, artifact) => {
                self.on_stage_completed(stage, artifact).await
            }
            WorkflowEvent::FailureOccurred(failure) => {
                self.on_failure_occurred(failure).await
            }
            WorkflowEvent::HumanIntervention(decision) => {
                self.on_human_intervention(decision).await
            }
            WorkflowEvent::ArtifactChanged(artifact) => {
                self.on_artifact_changed(artifact).await
            }
        }
    }
}

/// 触发器类型
enum WorkflowTrigger {
    /// 状态满足
    StateSatisfied(StateCondition),

    /// 产物变化
    ArtifactChanged(ArtifactId),

    /// 决策修订
    DecisionRevised(DecisionId),

    /// 人类介入产生 Invariant
    InvariantCreated(Invariant),
}

/// 产物注册表
struct ArtifactRegistry {
    // 追踪所有 Artifacts 及其版本
    artifacts: HashMap<ArtifactId, ArtifactVersion>,
}
```

---

## 5. 配置设计

```yaml
# NDC 工程配置 - 工业级优化版

engineering:
  # 阶段 0: 谱系继承
  lineage:
    enabled: true
    inherit_invariants: true
    inherit_failures: true
    max_context_depth: 3

  # 阶段 1: 需求理解
  understanding:
    timeout: 60
    similarity_threshold: 0.8

  # 阶段 2: 任务分解
  decomposition:
    max_subtasks: 20
    enable_lint: true  # 非LLM确定性校验
    lint_rules:
      - no_cyclic_dependency
      - all_verifiable
      - no_overly_broad

  # 阶段 3: 影子探测
  discovery:
    enabled: true
    required_for_high_volatility: true
    risk_threshold: 0.7

  # 阶段 4: 工作记忆
  working_memory:
    enabled: true
    max_recent_failures: 3
    auto_archive: true

  # 阶段 5: 执行
  execution:
    max_retries: 3
    quality_gates:
      - "cargo check"
      - "cargo test --lib"
      - "cargo clippy"

  # 阶段 6: 验收
  acceptance:
    require_human: false
    enhanced_for_high_risk: true
    coverage_threshold: 0.8

  # 阶段 7: 失败归因
  failure_handling:
    enable_taxonomy: true
    human_to_invariant: true
    invariant_priority: "highest"

# LLM 配置 - 模型自适应
llm:
  providers:
    fast:
      type: "gemini-flash"
      cost_per_token: 0.0001
    balanced:
      type: "gpt-4o-mini"
      cost_per_token: 0.0005
    strong:
      type: "gpt-4o"
      cost_per_token: 0.005

  selector:
    default_strategy: "risk-adjusted"
    fallback_provider: "balanced"
```

---

## 6. 状态流转总结

| 阶段 | 名称 | 触发条件 | 关键产物 | 新增特性 |
|------|------|---------|---------|---------|
| 0 | 谱系继承 | 新任务开始 | InheritedContext | ✅ |
| 1 | 理解需求 | 任务提交 | RequirementContext | - |
| 2 | 分解需求 | 理解完成 | TaskChain + DecompositionLint | ✅ 非LLM校验 |
| 3 | 影子探测 | 高Volatility | ImpactReport | ✅ Read-only |
| 4 | 工作记忆 | 探测完成 | WorkingMemory | ✅ 精简上下文 |
| 5 | 执行开发 | 记忆生成 | ExecutionResult | - |
| 6 | 验收 | 执行完成 | AcceptanceResult | 加强版验收 |
| 7 | 失败归因 | 验收失败 | Invariant + Postmortem | ✅ Human→Gold |
| 8 | 更新文档 | 验收通过 | DocumentChanges | Fact/Narrative |
| 9 | 完成 | 文档更新 | CompletionReport | 谱系更新 |

---

## 7. 实施优先级建议

### 第一刀：Discovery Phase (影子探测)
理由：
- Working Memory 的前置条件
- 风险控制第一道闸
- 工程收益立竿见影

### 第二刀：Working Memory + ContextSummarizer

### 第三刀：Human → Invariant → Gold Memory

---

## 8. 核心优势总结

| 维度 | 优化前 | 优化后 | 核心技术 |
|------|-------|-------|---------|
| **记忆** | 全量RAG | 动态Working Memory | 上下文剪裁 |
| **错误** | 简单重试 | Failure Taxonomy | 分类→策略 |
| **人类** | 阻塞处理 | Gold Memory | Invariant |
| **执行** | 直接修改 | 影子探测+影响分析 | Read-only |
| **资源** | 单一模型 | 自适应调度 | 路由算法 |
| **复用** | 孤立任务 | 谱系继承 | 知识传递 |

---

> **一句话总结**: NDC 已从"工程控制系统"进化到"工业级自治系统"——具备理解失败、记住教训、自动进化能力。
