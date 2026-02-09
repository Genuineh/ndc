# NDC 实现待办清单

> **重要更新 (2026-02-06)**: LLM 集成 - 知识驱动 + TODO 映射 + 工业级优化

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 + LLM Provider + TODO 管理 + Memory ✅
├── decision/          # [大脑] 决策引擎 ✅ 已完成
├── runtime/           # [身体] 执行与验证 + Workflow + Discovery ⏳
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon) ✅ 已完成
```

## 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────┐
│              NDC 工业级自治系统                                        │
│                                                                     │
│  知识库 ──▶ 理解需求 ──▶ TODO 映射 ──▶ 分解 ──▶ 影子探测 ──▶      │
│                                                                     │
│  工作记忆 ──▶ 执行开发 ──▶ 验收 ──▶ 失败归因 ──▶ 文档 ──▶ 完成     │
│                                                                     │
│  核心闭环: 人类纠正 → Invariant (Gold Memory) → 永不重复犯错          │
└─────────────────────────────────────────────────────────────────────┘
```

## 已完成模块 ✅

| 模块 | 文件 | 状态 | 说明 |
|------|------|------|------|
| **core** | task.rs | ✅ | Task, TaskState, ExecutionStep, ActionResult |
| **core** | intent.rs | ✅ | Intent, Verdict, PrivilegeLevel, Effect |
| **core** | agent.rs | ✅ | AgentRole, AgentId, Permission |
| **core** | memory.rs | ✅ | MemoryStability, MemoryQuery, MemoryEntry |
| **core** | config.rs | ✅ | YAML 配置系统 |
| **decision** | engine.rs | ✅ | DecisionEngine, validators |
| **runtime** | executor.rs | ✅ | Task execution, tool coordination |
| **runtime** | workflow.rs | ✅ | State machine, transitions |
| **runtime** | storage.rs | ✅ | In-memory storage |
| **runtime** | storage_sqlite.rs | ✅ | SQLite storage |
| **runtime** | tools/mod.rs | ✅ | Tool, ToolManager |
| **runtime** | tools/fs.rs | ✅ | File operations |
| **runtime** | tools/git.rs | ✅ | Git operations |
| **runtime** | tools/shell.rs | ✅ | Shell command execution |
| **runtime** | verify/mod.rs | ✅ | QualityGateRunner |
| **interface** | cli.rs | ✅ | CLI commands |
| **interface** | daemon.rs | ✅ | gRPC service framework |
| **interface** | grpc.rs | ✅ | gRPC service impl |
| **interface** | repl.rs | ✅ | REPL mode |
| **interface** | e2e_tests.rs | ✅ | E2E tests |
| **interface** | grpc_client.rs | ✅ | gRPC client SDK |

---

## LLM 集成 - 知识驱动 + 工业级自治 ⏳

```
📄 详细设计: docs/ENGINEERING_CONSTRAINTS.md

九大阶段:
0. 谱系继承 → 继承历史知识
1. 理解需求 → 检索知识库 + 检查 TODO
2. 建立映射 → 关联/创建总 TODO
3. 分解任务 → LLM 分解 + 非LLM确定性校验
4. 影子探测 → Read-Only 影响分析 ← ✅ P1 已完成
5. 工作记忆 → 精简上下文 ← P2
6. 执行开发 → 质量门禁 + 重来机制
7. 失败归因 → Human Correction → Invariant ← P3
8. 更新文档 → Fact/Narrative
9. 完成 → 谱系更新
```

### 工业级优化组件 ⏳

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 组件                     │ 文件                          │ 优先级       │
├─────────────────────────────────────────────────────────────────────────────┤
│ Working Memory           │ memory/working_memory.rs     │ P2 ✅ DONE  │
│ Discovery Phase          │ discovery/mod.rs             │ P1 ✅ DONE  │
│ Failure Taxonomy        │ error/taxonomy.rs            │ P2 ✅ DONE  │
│ Invariant (Gold Memory) │ memory/invariant.rs          │ P3 ✅ DONE  │
│ Model Selector           │ llm/selector.rs             │ P3 ✅ DONE  │
│ Task Lineage            │ todo/lineage.rs              │ P2 ✅ DONE  │
│ Event-Driven Engine     │ engine/mod.rs               │ P3 ✅ DONE  │
│ Decomposition Lint      │ llm/decomposition/lint.rs    │ P2 ✅ DONE  │
└─────────────────────────────────────────────────────────────────────────────┘

P1 = 第一刀 (Discovery Phase) - ✅ 已验收通过 (ec499ab)
P2 = 第二刀 (Working Memory + Saga) - ✅ 已完成
P3 = 第三刀 (Invariant + Telemetry) - ✅ 已完成
```

---

## 代码结构 (规划中)

```
crates/core/src/
├── llm/
│   ├── mod.rs              # Provider Trait
│   ├── provider/
│   │   ├── mod.rs          # Trait 定义
│   │   ├── openai.rs       # OpenAI
│   │   ├── anthropic.rs     # Anthropic
│   │   └── minimax.rs       # MiniMax
│   ├── understanding.rs     # 阶段 1
│   ├── decomposition/
│   │   ├── mod.rs          # 分解服务
│   │   ├── planner.rs      # 任务规划
│   │   └── lint.rs         # 非LLM校验 ⭐
│   └── selector.rs          # 模型自适应 ⭐
│
├── todo/
│   ├── mod.rs              # TODO 模块
│   ├── project_todo.rs     # 总 TODO
│   ├── task_chain.rs       # 任务链
│   ├── mapping_service.rs   # 映射服务
│   └── lineage.rs          # 谱系继承 ⭐
│
├── memory/                 # ✅ P2 Working Memory 已完成
│   ├── mod.rs
│   ├── knowledge_base.rs    # 知识库
│   ├── working_memory.rs   # WorkingMemory ⭐
│   └── invariant.rs        # Gold Memory ⭐ P3
│
└── error/
    └── taxonomy.rs         # 失败分类 ⭐

crates/runtime/src/
├── engine/
│   ├── mod.rs              # 事件驱动引擎 ⭐
│   ├── workflow.rs         # 工作流
│   ├── execution.rs        # 执行引擎
│   └── acceptance.rs       # 验收
│
├── discovery/              # ✅ P1 已完成
│   ├── mod.rs              # DiscoveryService
│   ├── heatmap.rs          # VolatilityHeatmap
│   ├── hard_constraints.rs  # HardConstraints
│   └── impact_report.rs    # ImpactReport
│
├── execution/              # ✅ P2 Saga Pattern 已完成
│   └── mod.rs              # SagaPlan, UndoAction
│
└── documentation/
    └── updater.rs         # 文档更新
```

---

## 实施优先级

### ⭐ 第一刀：Discovery Phase (影子探测) ✅ 已验收通过

```
职责: 在动手前先照 X 光
触发: 高 Volatility 模块
产物: ImpactReport + HardConstraints

核心约束:
- 只读扫描 (fs read / grep / ls)
- 禁止写文件 / git commit
- 高风险 → 触发加强版验收

配置:
discovery:
  enabled: true
  risk_threshold: 0.7
```

**验收标准**:
- [x] ImpactReport 结构 (impact_report.rs:ImpactReport)
- [x] VolatilityScore 计算 (heatmap.rs:VolatilityHeatmap)
- [x] Hard Constraints 生成 (hard_constraints.rs:HardConstraints)
- [x] 强制回归测试注入 (hard_constraints.rs:RegressionTest)
- [x] 隐性耦合检测 (hard_constraints.rs:CouplingWarning)
- [x] 触发加强验收逻辑 (mod.rs:should_generate_constraints)

**测试覆盖**: 15/15 通过

**实现文件**:
- crates/runtime/src/discovery/mod.rs (DiscoveryService)
- crates/runtime/src/discovery/heatmap.rs (VolatilityHeatmap)
- crates/runtime/src/discovery/hard_constraints.rs (HardConstraints)
- crates/runtime/src/discovery/impact_report.rs (ImpactReport)

**提交**: ec499ab feat: 实现 Discovery Phase (P1) - 波动热力图 + 硬约束

---

### 第二刀：Working Memory + ContextSummarizer

```
职责: 执行态认知边界
特点:
- 强生命周期 (SubTask 结束时销毁)
- 非检索型 (系统喂给 LLM)
- 工程优先 (API > 约束 > 文档)

包含:
- active_files
- api_surface
- recent_failures (最近 3 次)
- invariants (Gold Memory)
```

---

### 第三刀：Human → Invariant → Gold Memory

```
职责: "同一个坑填过一次，永远不会再掉进去"

流程:
1. 人类纠正错误
2. 分类: FailureTaxonomy::HumanCorrection
3. 抽象为 FormalConstraint
4. 注入 Gold Memory
5. 影响:
   - Future WorkingMemory
   - Decomposition Validator
   - ModelSelector (高风险)

优先级: Highest (人类纠正 > 系统推理 > LLM 建议)
```

---

## 核心数据结构

### Failure Taxonomy

```rust
enum FailureTaxonomy {
    LogicError,           // 重试
    TestGap,              // 重试
    SpecAmbiguity,        // 回阶段1
    DecisionConflict,     // 回阶段2
    ToolFailure,          // 视情况
    HumanCorrection,      // 产生 Invariant
}
```

### Task Lineage

```rust
struct TaskLineage {
    parent: Option<TaskId>,
    inherited_invariants: Vec<InvariantRef>,
    inherited_failures: Vec<FailurePattern>,
    inherited_context: Option<ArchivedWorkingMemory>,
}
```

### Model Selector

```rust
fn select_model(entropy: TaskEntropy) -> LlmProvider {
    // 低风险 + 高不变量密度 → 快速模型
    // 中等风险 → 均衡模型
    // 高风险 / 跨模块 → 最强模型
}
```

---

## 快速开始

```bash
# 检查编译状态
cargo check

# 运行所有测试
cargo test

# 构建二进制
cargo build

# 运行 CLI
./target/debug/ndc --help

# 运行 REPL
./target/debug/ndc repl

# 创建任务
./target/debug/ndc create "test task" -d "description"
```

---

## 下一步工作

### 短期 (P1) - ✅ 已完成
- [x] Discovery Phase 实现 (crates/runtime/src/discovery/)
- [x] ImpactReport 结构 (impact_report.rs)
- [x] VolatilityScore 计算 (heatmap.rs)
- [x] Hard Constraints 生成
- [x] Read-only Tool 限制

### 中期 (P2) - ✅ 已完成
- [x] Working Memory 设计 (crates/core/src/memory/working_memory.rs)
- [x] Saga Pattern 实现 (crates/runtime/src/execution/mod.rs)
- [x] Task Lineage 继承 (todo/lineage.rs)
- [x] Decomposition Lint (llm/decomposition/lint.rs)

**Working Memory 测试**: 5/5 通过
**Saga Pattern 测试**: 7/7 通过
**Lineage 测试**: 5/5 通过
**Decomposition Lint 测试**: 5/5 通过

**实现文件**:
- crates/core/src/memory/working_memory.rs (WorkingMemory, AbstractHistory, LlmContext)
- crates/runtime/src/execution/mod.rs (SagaPlan, SagaStep, UndoAction, CompensationAction)

### 长期 (P3) - ✅ 已完成
- [x] Invariant Gold Memory (memory/invariant.rs)
- [x] Model Selector (llm/selector.rs)
- [x] Event-Driven Engine (engine/mod.rs)

**P3 测试覆盖**: 20/20 通过
- Invariant Gold Memory: 7/7 测试通过
- Model Selector: 9/9 测试通过
- Event-Driven Engine: 8/8 测试通过

**实现文件**:
- crates/core/src/memory/invariant.rs (GoldMemory, GoldInvariant, GoldMemoryService)
- crates/core/src/llm/selector.rs (ModelSelector, TaskCharacteristics, LlmProvider)
- crates/runtime/src/engine/mod.rs (EventEngine, EventEmitter, Workflow)

---

最后更新: 2026-02-09 (P3 已完成 - Invariant Gold Memory + Model Selector + Event-Driven Engine)
标签: #ndc #llm #industrial-grade #autonomous #p1-complete #p2-complete #p3-complete
