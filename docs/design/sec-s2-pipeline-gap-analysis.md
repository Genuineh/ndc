# SEC-S2 — 10 阶段管线缺口评估

> 对照 `docs/ENGINEERING_CONSTRAINTS.md` 设计文档与实际代码实现的差距分析。

## 总览

| 阶段 | 名称 | 设计状态 | 实现状态 | 测试 | 评估 |
|------|------|---------|---------|------|------|
| 0 | Lineage Inheritance | ✅ 已设计 | ✅ 已实现 | 5 | 完成 |
| 1 | Understand | ✅ 已设计 | ⚠️ 部分实现 | 1 | 结构在，未集成 |
| 2 | Decompose | ✅ 已设计 | ⚠️ 部分实现 | 5 | Lint 完整，缺 Undo Plan |
| 3 | Discovery | ✅ 已设计 | ✅ 已实现 | 17 | 完成 |
| 4 | Working Memory | ✅ 已设计 | ✅ 已实现 | 7 | 完成 |
| 5 | Develop (Saga) | ✅ 已设计 | ✅ 已实现 | 8 | 完成 |
| 6 | Accept | ✅ 已设计 | ⚠️ 部分实现 | 1 | 基础验证在，缺覆盖率门禁 |
| 7 | Failure → Invariant | ✅ 已设计 | ❌ 未实现 | 0 | 关键缺口 |
| 8 | Document | ✅ 已设计 | ⚠️ 部分实现 | 2 | Fact/Narrative 在，未写磁盘 |
| 9 | Complete | ✅ 已设计 | ❌ 未实现 | 0 | 关键缺口 |

**完成度**: 4/10 完整 + 4/10 部分 + 2/10 缺失 ≈ 60%

---

## 当前工作流阶段映射

`AgentWorkflowStage` 枚举（`crates/core/src/ai_agent/mod.rs:127`）定义了 5 个阶段：

| 工作流阶段 | 对应设计阶段 |
|-----------|------------|
| Planning | Stage 0 (Lineage) + Stage 1 (Understand) + Stage 2 (Decompose) |
| Discovery | Stage 3 (Discovery) + Stage 4 (Working Memory) |
| Executing | Stage 5 (Develop) |
| Verifying | Stage 6 (Accept) |
| Completing | Stage 8 (Document) + Stage 9 (Complete) |

**缺失映射**: Stage 7 (Failure → Invariant) 无对应阶段。

---

## 各阶段详细分析

### Stage 0: Lineage Inheritance — ✅ 完成

**实现**: `crates/core/src/todo/lineage.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| 检查父任务历史 | ✅ `TaskLineage` 跟踪 parent-child 关系 |
| 继承 Invariants (带版本标签) | ✅ `InheritedInvariant` 结构 |
| 继承 PostmortemContext | ✅ `ArchivedContext` 包含失败上下文 |
| 继承 Undo Plans | ⚠️ Lineage 结构存在但 SagaPlan 未通过 Lineage 传递 |
| 输出 InheritedContext | ✅ 完整输出 |

**测试**: 5 个单元测试。

**差距**: Undo Plan 继承路径未打通（SagaPlan 独立于 Lineage）。

---

### Stage 1: Understand — ⚠️ 部分实现

**实现**: `crates/core/src/llm/understanding.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| 检索知识库 (含 Heatmap) | ❌ Heatmap 在 Stage 3 生成，未回注 Stage 1 |
| 检索总 TODO | ❌ 未实现 |
| 谱系继承 | ⚠️ Lineage 模块存在但未在 Understanding 中调用 |
| Volatility 评估 | ❌ 未实现 |
| 输出 RequirementContext | ⚠️ 有 `UnderstandingContext` 但不含 Volatility 信息 |

**已有类型**:
- `Requirement`, `RequirementIntent`, `Entity`, `Relationship`, `Constraint`
- `UnderstandingContext`, `KnowledgeItem`, `KnowledgeUnderstandingService`
- `UnderstandingConfig` 配置

**测试**: 1 个。

**差距**: 数据结构定义完整，但缺少与知识库、Heatmap、Lineage 的集成逻辑。Understanding 是孤岛状态。

---

### Stage 2: Decompose — ⚠️ 部分实现

**实现**: `crates/core/src/llm/decomposition/lint.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| LLM 理解需求 | ⚠️ `TaskDecomposition` 结构在但 LLM 调用未接入 |
| 分解为原子子任务 | ✅ `SubTask` + `ActionType` + `Complexity` |
| 创建子 TODO 链 | ⚠️ 结构在但未与 TODO 系统联通 |
| 分解 Lint 校验 | ✅ 6 条规则: CyclicDependency, MissingVerification, TooComplex, OrphanedTask, MissingFiles, TooManySubtasks |
| Undo Plan 生成 | ❌ 未实现 |

**测试**: 5 个（Lint 规则测试）。

**差距**: Lint 系统完善度高；缺少 LLM 调用集成和 Undo Plan 生成。

---

### Stage 3: Discovery Phase — ✅ 完成

**实现**: `crates/runtime/src/discovery/`
- `hard_constraints.rs` — HardConstraints, CouplingWarning, CouplingType
- `heatmap.rs` — VolatilityHeatmap, git 历史分析
- `impact_report.rs` — ImpactReport, 只读影响分析
- `mod.rs` — 模块组织

| 设计要求 | 实现状态 |
|---------|---------|
| Read-Only Impact Analysis | ✅ `ImpactReport` |
| 生成 Volatility Heatmap | ✅ `VolatilityHeatmap::from_git()` |
| 转化 Hard Constraints | ✅ `HardConstraints::inject_into_quality_gate()` |
| 隐性耦合检测 | ✅ `CouplingWarning` + 4 种耦合类型 |
| 评估 VolatilityRisk | ✅ `mark_high_risk()` |

**测试**: 17 个单元测试，覆盖全面。

**差距**: 无。此阶段是实现最完整的模块之一。

---

### Stage 4: Working Memory — ✅ 完成

**实现**: `crates/core/src/memory/working_memory.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| Abstract(History) | ✅ `AbstractHistory` + `FailurePattern` + `TrajectoryState` |
| Raw(Current) | ✅ `RawCurrent` (active_files, api_surface, step_context) |
| Hard(Invariants) | ✅ `VersionedInvariant` 列表 |
| Cycle Detection | ✅ `detect_cycle()` 方法 |
| SubTask 作用域 | ✅ `scope: SubTaskId` |
| 结束即销毁/归档 | ⚠️ 设计原则在，自动归档逻辑未编码 |

**测试**: 7 个单元测试。

**差距**: 自动归档策略需在 orchestrator 集成层实现。

---

### Stage 5: Develop (Saga) — ✅ 完成

**实现**: `crates/runtime/src/execution/mod.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| 子任务循环 | ✅ 通过 orchestrator 工具调用循环实现 |
| 质量门禁 | ✅ `QualityGateRunner` (cargo check/test/clippy) |
| Saga Rollback | ✅ `SagaPlan`, `UndoAction`, `CompensationAction` |
| 失败分类 | ❌ 未实现 (Stage 7) |
| 重来机制 | ⚠️ orchestrator 有 max_retries 但未接入 Saga |
| 人工介入 | ⚠️ 通过 REPL Ask 权限机制间接实现 |

**SagaPlan 支持的 UndoAction**:
- `DeleteFile` — 删除文件
- `RestoreFile` — 恢复文件（含备份内容）
- `GitRevert` — Git 回滚
- `RunCleanupCommand` — 清理命令

**测试**: 8 个单元测试。

**差距**: Saga 模块完整但未与 orchestrator 重试循环打通。

---

### Stage 6: Accept — ⚠️ 部分实现

**实现**: `crates/runtime/src/verify/mod.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| 自动验收 (覆盖率 >= 80%) | ❌ 缺少覆盖率度量 |
| 所有测试通过 | ✅ `run_tests()` |
| 人工验收 | ❌ 未区分自动/人工 |
| 强制回归测试 | ⚠️ `run_with_constraints()` 存在但调用链不完整 |
| VolatilityRisk 触发加强版验收 | ❌ 未实现 |
| 输出 AcceptanceResult | ❌ 使用 `QualityResult` 而非 `AcceptanceResult` |

**已有功能**:
- `QualityGateRunner::run()` — 运行 cargo check/test/clippy
- `run_with_constraints()` — 带 HardConstraints 的验收
- `run_security_check()` — 安全检查
- `run_build()` — 构建检查

**测试**: 1 个。

**差距**: 基础验证齐全，缺覆盖率门禁、人工/自动区分、风险自适应验收。

---

### Stage 7: Failure → Invariant — ❌ 未实现

**实现**: 无

| 设计要求 | 实现状态 |
|---------|---------|
| 失败分类 (FailureTaxonomy) | ❌ |
| NonDeterministic Failure 检测 | ❌ |
| Human Correction → Invariant | ❌ |
| PostmortemContext 生成 | ❌ |
| Invariant 冲突检测 | ❌ |
| 回灌到 KnowledgeBase | ❌ |

**计划路径**: `crates/core/src/error/taxonomy.rs`

**依赖**: Invariant 系统 (`crates/core/src/memory/invariant.rs`) 已完整（9 个测试），可作为此阶段的基础。`GoldInvariant` 和 `VersionedInvariant` 已支持 TTL、版本标签、来源追踪（含 `HumanCorrection`）。

**差距**: 这是最大的功能缺口。失败后的学习闭环完全缺失，导致系统无法从错误中自我进化。

---

### Stage 8: Document — ⚠️ 部分实现

**实现**: `crates/runtime/src/documentation/mod.rs`

| 设计要求 | 实现状态 |
|---------|---------|
| Fact Docs | ✅ `Fact` + `FactCategory` (Architecture/Api/Behavior/Config/Dependency/Performance) |
| Narrative Docs | ✅ `Narrative` 结构 |
| 决策记录 | ⚠️ Fact 可表达但无专用 DecisionRecord 类型 |
| 提升知识库稳定性 | ❌ 未实现稳定性层级更新 |
| 更新 Undo Plan | ❌ 未与 Saga 联通 |
| 输出 DocumentChanges | ⚠️ `DocUpdateResult` 存在但未实际写入文件 |

**已有功能**:
- `DocUpdater::record_fact()` — 记录事实
- `DocUpdater::get_facts_by_category()` — 按分类查询
- `DocUpdater::generate_narrative()` — 生成叙述文档

**测试**: 2 个。

**差距**: 内存中数据模型完整，缺少持久化写入和与 Saga/Invariant 的集成。

---

### Stage 9: Complete — ❌ 未实现

**实现**: 无

| 设计要求 | 实现状态 |
|---------|---------|
| 标记总 TODO 完成 | ❌ |
| 发送完成通知 | ❌ |
| Telemetry 更新 | ❌ |
| TaskLineage 更新 | ❌ |
| 输出 CompletionReport | ❌ |

**计划路径**: `crates/core/src/telemetry/mod.rs`

**差距**: 遥测系统完全缺失。无法量化自主率、人工介入成本、Token 效率等核心指标。

---

## 跨阶段集成缺口

### 1. 数据流断裂

```
Stage 1 (Understand) ← ✗ ← Stage 3 (Discovery/Heatmap)
   Heatmap 在 Stage 3 生成，但 Stage 1 无法消费

Stage 5 (Develop) ← ✗ ← Stage 5 (Saga)
   SagaPlan 定义完整但未注入 orchestrator 重试循环

Stage 7 (Failure) ← ✗ ← Stage 6 (Accept)
   验收失败后无失败分类，直接重试或放弃

Stage 0 (Lineage) ← ✗ ← Stage 9 (Complete)
   无 CompletionReport 回写到 Lineage 供后续任务继承
```

### 2. Orchestrator 集成

`AgentOrchestrator::run_agent_loop()`（`crates/core/src/ai_agent/orchestrator.rs`）是主执行引擎，当前阶段切换:

```
Planning → Discovery → Executing (loop) → Verifying → Completing
```

**缺失**:
- Planning 未调用 Lineage/Understanding/Decomposition 模块
- Discovery 未调用 `VolatilityHeatmap` / `HardConstraints`
- Executing 未使用 `WorkingMemory` 或 `SagaPlan`
- Verifying 未使用 `HardConstraints::inject_into_quality_gate()`
- Completing 无 Document/Telemetry 逻辑

### 3. Invariant 系统孤岛

`crates/core/src/memory/invariant.rs` 实现完整（`GoldInvariant`, `VersionedInvariant`, TTL, 版本标签, 9 个测试），但：
- 未被 orchestrator 在执行前查询
- 未被 Failure 阶段写入
- 未被 Lineage 阶段继承
- 未在 WorkingMemory 的 `hard_invariants` 中实际填充

---

## 建议：收敛还是补齐

### 推荐策略: **渐进补齐 + 设计收敛**

1. **第一优先 (P0)**: 打通 orchestrator → 已实现模块的调用链
   - Planning 调用 Lineage + Understanding
   - Discovery 调用 Heatmap + HardConstraints
   - Verifying 调用 `run_with_constraints()`
   - 这些模块代码已存在，只需在 orchestrator 中接入

2. **第二优先 (P1)**: 实现 Stage 7 (Failure → Invariant)
   - 依赖 Invariant 系统已就绪
   - 是自治闭环的核心（从失败中学习）
   - 建议路径: `crates/core/src/error/taxonomy.rs`

3. **第三优先 (P2)**: 实现 Stage 9 (Complete + Telemetry)
   - 提供可观测性指标
   - 建议路径: `crates/core/src/telemetry/mod.rs`

4. **可延后**: 覆盖率门禁、人工/自动区分验收、文档持久化写入
   - 这些是"锦上添花"功能，不影响核心闭环

### 设计文档调整

`ENGINEERING_CONSTRAINTS.md` 中的伪代码（如 `pub隐性_coupling_warnings`, `pub fn精简_context_for_llm`）包含中文标识符，应在实际实现中使用英文标识符（已在各 crate 中正确实现）。设计文档本身作为愿景参考保留即可，无需强制对齐。

---

## 测试覆盖总结

| 模块 | 测试数 | 覆盖评估 |
|------|-------|---------|
| Lineage | 5 | 良好 |
| Understanding | 1 | 不足 |
| Decomposition Lint | 5 | 良好 |
| Discovery (Heatmap + HC) | 17 | 优秀 |
| Working Memory | 7 | 良好 |
| Saga/Execution | 8 | 良好 |
| Invariant (Gold Memory) | 9 | 优秀 |
| QualityGate | 1 | 不足 |
| Documentation | 2 | 不足 |
| Failure Taxonomy | 0 | 缺失 |
| Telemetry | 0 | 缺失 |
| **Total pipeline-related** | **55** | — |

---

*Created: SEC-S2 pipeline gap assessment*
*Ref: `docs/ENGINEERING_CONSTRAINTS.md`, `crates/core/src/ai_agent/mod.rs:127`*
