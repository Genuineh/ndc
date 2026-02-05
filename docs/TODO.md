# NDC 实现待办清单

> **重要更新 (2026-02-05)**: 采用 **NDC 2.0 深度融合方案（优化版）**
> - 放弃 Adapter 层，DevMan 功能"器官化"整合
> - 详情见: `docs/devman-integration-plan.md`
> - 新增：依赖循环防范、Type Alias 策略、Snapshot 支持、事务存储

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 (Task-Intent 合一) ✅ 已更新
├── decision/          # [大脑] 决策引擎
├── cognition/         # [记忆] 认知网络 (原 DevMan Knowledge)
├── runtime/           # [身体] 执行与验证 (Tools + Quality)
├── persistence/       # [归档] 存储层（含事务）
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon)
```

## ✅ 已完成

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| - | `crates/core/src/task.rs` | ✅ | Task-Intent 统一，含 Snapshot |
| - | `crates/core/src/intent.rs` | ✅ | Intent, Verdict, Effect |
| - | `crates/core/src/agent.rs` | ✅ | AgentRole, Permission |
| - | `crates/core/src/memory.rs` | ✅ | MemoryStability, MemoryQuery |

---

## Phase 1: 内核重构 (Week 1) [P0]

### 1.1 ndc-core 核心数据 ✅ 已更新
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | `crates/core/Cargo.toml` | ☐ | 更新依赖（ulid, chrono, serde） |
| P0 | `crates/core/src/task.rs` | ✅ | **含 Snapshot** |
| P0 | `crates/core/src/intent.rs` | ✅ | Intent, Verdict, Effect |
| P0 | `crates/core/src/agent.rs` | ✅ | AgentRole, Permission |
| P0 | `crates/core/src/memory.rs` | ✅ | MemoryStability |

### 1.2 ndc-persistence 存储层
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | `crates/persistence/Cargo.toml` | ☐ | 创建存储 crate |
| P0 | `crates/persistence/src/store.rs` | ☐ | **存储抽象 trait（含事务）** |
| P0 | `crates/persistence/src/json.rs` | ☐ | JSON 实现 |
| P0 | `crates/persistence/src/lib.rs` | ☐ | 模块入口 |

### 1.3 ndc-decision 决策引擎
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | `crates/decision/Cargo.toml` | ☐ | 创建决策 crate |
| P0 | `crates/decision/src/engine.rs` | ☐ | DecisionEngine trait |
| P0 | `crates/decision/src/validators.rs` | ☐ | 内置校验器 |

---

## Phase 2: 执行层吸收 (Week 2) [P0]

### 2.1 ndc-runtime 执行引擎
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | `crates/runtime/Cargo.toml` | ☐ | 创建执行 crate |
| P0 | `crates/runtime/src/executor.rs` | ☐ | 异步任务调度器 |
| P0 | `crates/runtime/src/workflow.rs` | ☐ | 状态机 |
| P0 | `crates/runtime/src/tools/` | ☐ | 受控工具集 |
| P0 | `crates/runtime/src/verify/` | ☐ | 质量门禁 |

---

## Phase 3: 认知升级 (Week 3) [P1]

### 3.1 ndc-cognition 认知网络
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P1 | `crates/cognition/Cargo.toml` | ☐ | 创建认知 crate |
| P1 | `crates/cognition/src/lib.rs` | ☐ | 模块入口 |
| P1 | `crates/cognition/src/vector.rs` | ☐ | 向量检索 (#Issue 3) |
| P1 | `crates/cognition/src/stability.rs` | ☐ | 记忆稳定性 (#Issue 1) |
| P1 | `crates/cognition/src/context.rs` | ☐ | 上下文组装 |

---

## Phase 4: 交互层 (Week 4) [P2]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P2 | `crates/interface/Cargo.toml` | ☐ | 创建交互 crate |
| P2 | `crates/interface/src/cli.rs` | ☐ | CLI 入口 |
| P2 | `crates/interface/src/repl.rs` | ☐ | REPL 模式 |
| P2 | `crates/interface/src/daemon.rs` | ☐ | gRPC 服务 |

---

## DevMan 迁移清单

| 来源 | 目标 | 状态 |
|------|------|------|
| devman-core | ndc-core | ✅ 已更新 |
| devman-storage | ndc-persistence | 待迁移 |
| devman-tools | ndc-runtime/tools | 待迁移 |
| devman-quality | ndc-runtime/verify | 待迁移 |
| devman-knowledge | ndc-cognition | 待迁移 |
| devman-work | ndc-runtime/workflow | 待迁移 |
| devman-ai | ndc-interface | 待迁移 |
| devman-progress | ndc-runtime | 待迁移 |

---

## 核心原则检查

- [x] **向下引用**: `core` 是纯数据，不引用其他 crate
- [x] **Type Alias**: 迁移时使用别名兼容旧代码
- [x] **Snapshot**: Task 包含 `snapshots` 支持回滚
- [x] **事务**: 存储层支持 `Transaction` trait

---

## 插件系统 [P3]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P3 | MCP 协议支持 | ☐ | Model Context Protocol |
| P3 | Skills 系统 | ☐ | 工作流模板 |
| P3 | WASM 沙箱 | ☐ | 自定义扩展 |

---

## 外部依赖 (DevMan Issues)

| Issue | 功能 | 状态 | 优先级 |
|-------|------|------|--------|
| #1 | 知识稳定性 | 已集成规划 | 中 |
| #2 | 访问控制 | 待规划 | 中 |
| #3 | 向量检索 | 已集成规划 | 高 |

---

## 优先级说明

| 标记 | 含义 |
|------|------|
| P0 | 必须完成，MVP 核心 |
| P1 | 重要功能，后续阶段依赖 |
| P2 | 增强体验，可后续 |
| P3 | 优化项，可选实现 |

---

## 相关文档

- `docs/devman-integration-plan.md` - 深度融合详细方案（优化版）
- `docs/design/2026-02-04-ndc-final-design.md` - 架构设计

---

最后更新: 2026-02-05
标签: #ndc #todo #integration
