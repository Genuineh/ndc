# NDC 与 DevMan 集成设计文档

**项目名称**: NDC (Neo Development Companion)
**版本**: v1.0
**日期**: 2026-02-04
**状态**: 设计阶段

---

## 1. 概述

### 1.1 设计目标

将 DevMan 作为执行后端集成到 NDC，同时保持 NDC 的架构独立性。

- **NDC 核心**：Decision Engine（决策与约束引擎）
- **DevMan 后端**：Task/Quality/Tools/Knowledge/Storage
- **集成方式**：Adapter 模式（领域转换层）

### 1.2 集成范围

| 功能 | 实现方 |
|------|--------|
| Decision Engine | NDC 独有实现 |
| Task 管理 | DevMan |
| 质量引擎 | DevMan |
| 工具执行 | DevMan |
| 知识服务 | DevMan (扩展规划) |
| 存储后端 | DevMan |
| 向量检索 | 提需求给 DevMan |
| 访问控制 | 提需求给 DevMan |
| 记忆稳定性 | 提需求给 DevMan |

---

## 2. 整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                        NDC 架构（Adapter 模式）                      │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  Decision Engine (NDC 独有核心)                              │   │
│  │  • Intent 评估  • Verdict 裁决  • 约束校验                  │   │
│  └────────────────────────────────┬────────────────────────────┘   │
│                                   │                                  │
│  ┌────────────────────────────────▼────────────────────────────┐   │
│  │              NDC 领域层                                       │   │
│  │  • Intent/Verdict/Effect  • Agent/AgentRole                 │   │
│  │  • MemoryStability  • Permission                            │   │
│  └────────────────────────────────┬────────────────────────────┘   │
│                                   │                                  │
│  ┌────────────────────────────────▼────────────────────────────┐   │
│  │              Adapter 层（领域转换）                           │   │
│  │  • NDC Intent → DevMan Task  • NDC Verdict → 权限检查       │   │
│  │  • NDC Memory → DevMan Knowledge                             │   │
│  └────────────────────────────────┬────────────────────────────┘   │
│                                   │                                  │
│  ┌────────────────────────────────▼────────────────────────────┐   │
│  │              DevMan（执行后端）                               │   │
│  │  ✅ Task/Quality/Tools/Knowledge/Storage                     │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.1 核心原则

1. **NDC 拥有独立的领域模型**：Intent、Verdict、AgentRole、MemoryStability
2. **DevMan 作为执行后端**：Task 管理、质量检查、工具执行
3. **Adapter 负责双向转换**：NDC 模型 ↔ DevMan 模型
4. **Decision Engine 是守门员**：所有操作必须先通过裁决

---

## 3. NDC 核心层

### 3.1 Decision Engine（NDC 独有）

**职责**：
- 评估 Agent 的 Intent 提案
- 返回 Verdict 裁决（Allow/Deny/RequireHuman/Modify/Defer）
- 执行约束校验

```rust
#[async_trait]
pub trait DecisionEngine: Send + Sync {
    async fn evaluate(&self, intent: Intent) -> Verdict;
    fn register_validator(&mut self, validator: Box<dyn Validator>);
    fn policy_state(&self) -> PolicyState;
}
```

### 3.2 NDC 领域模型

| 类型 | 说明 | DevMan 对应 |
|------|------|------------|
| Intent | AI 的意图提案 | 无（NDC 独有） |
| Verdict | 系统裁决 | 无（NDC 独有） |
| AgentRole | Planner/Implementer/Reviewer/Tester/Historian | 无（NDC 独有） |
| MemoryStability | Ephemeral/Derived/Verified/Canonical | 无（规划中） |
| Permission | 基于 AgentRole 的权限 | 无（规划中） |

---

## 4. Adapter 层设计

### 4.1 任务适配器

```rust
pub struct TaskAdapter {
    devman_task: Arc<DevManTaskManager>,
    decision_engine: Arc<DecisionEngine>,
}

impl TaskAdapter {
    pub async fn create_intent(&self, intent: Intent) -> Result<TaskId> {
        // 1. Decision Engine 评估
        let verdict = self.decision_engine.evaluate(intent.clone()).await?;

        // 2. 根据裁决处理
        match verdict {
            Verdict::Allow => {
                // 转换为 DevMan Task
                let devman_task = self.to_devman_task(intent)?;
                self.devman_task.create(devman_task).await
            }
            Verdict::Deny { reason, .. } => Err(Error::Denied(reason)),
            Verdict::RequireHuman { question, .. } => Err(Error::HumanRequired(question)),
            // ...
        }
    }
}
```

### 4.2 领域映射

| NDC 概念 | DevMan 概念 | 转换逻辑 |
|----------|-------------|---------|
| Intent | Task + WorkRecord | Intent 提案 → DevMan Task 创建 |
| AgentRole | Task assignee | Planner → DevMan Task 分配 |
| MemoryEntry | Knowledge | MemoryEntry → DevMan Knowledge |
| QualityCheck | devman_run_quality_check | 直接映射 |
| ToolExecution | devman_execute_tool | 直接映射 |

---

## 5. DevMan 功能映射

### 5.1 直接复用

| DevMan 功能 | NDC 接口 | 说明 |
|-------------|---------|------|
| `devman_create_task` | `TaskAdapter::create()` | 通过 Decision Engine 裁决后创建 |
| `devman_get_task_guidance` | `TaskAdapter::get_guidance()` | 获取任务状态和下一步引导 |
| `devman_run_quality_check` | `QualityAdapter::check()` | 编译/测试/lint 检查 |
| `devman_execute_tool` | `ToolAdapter::execute()` | cargo/git/npm/fs/bash 执行 |
| `devman_search_knowledge` | `KnowledgeAdapter::search()` | 知识检索 |
| `devman_save_knowledge` | `KnowledgeAdapter::save()` | 保存知识 |
| `JobManager` | `AsyncTaskAdapter` | 异步任务管理 |

### 5.2 状态映射

| NDC TaskState | DevMan TaskState | 说明 |
|---------------|-----------------|------|
| Pending | Created | 初始状态 |
| Preparing | ContextRead | 准备中 |
| InProgress | InProgress | 执行中 |
| AwaitingVerification | QualityChecking | 等待质检 |
| Completed | Completed | 已完成 |
| Failed | Abandoned | 失败/放弃 |

---

## 6. 待提交给 DevMan 的功能需求

### 6.1 向量检索

**需求描述**：为知识服务添加语义搜索能力

**建议实现**：
- 在 KnowledgeService 中集成向量数据库（Qdrant）
- 为保存的知识自动生成 embedding
- 支持相似度搜索

**Issue 模板**：
```markdown
## Feature: 向量检索支持知识服务

### 问题描述
当前知识服务基于关键词搜索，无法理解语义相似性。

### 建议方案
- 集成 Qdrant 作为向量存储
- 使用 OpenAI/Claude API 生成 embedding
- 添加 `search_by_vector()` 方法

### 优先级
高
```

### 6.2 访问控制

**需求描述**：为知识服务添加基于角色的访问控制

**建议实现**：
```rust
pub struct AccessControl {
    pub owner: AgentId,
    pub read_roles: HashSet<AgentRole>,
    pub write_roles: HashSet<AgentRole>,
}
```

### 6.3 记忆稳定性

**需求描述**：知识条目添加稳定性等级，区分临时结论和已验证事实

**建议实现**：
```rust
pub enum KnowledgeStability {
    Ephemeral,   // 临时推理
    Derived,     // 推导结论
    Verified,    // 已验证
    Canonical,   // 事实/约束
}
```

---

## 7. 项目结构

```
ndc/
├── Cargo.toml                 # 添加 DevMan git 依赖
├── crates/
│   ├── core/                  # NDC 核心数据模型
│   │   ├── src/
│   │   │   ├── intent.rs      # Intent, Verdict, Effect
│   │   │   ├── agent.rs       # Agent, AgentRole, Permission
│   │   │   └── memory.rs      # MemoryEntry, MemoryStability
│   │   └── Cargo.toml
│   │
│   ├── decision/              # Decision Engine（NDC 独有）
│   │   ├── src/
│   │   │   ├── engine.rs      # DecisionEngine trait
│   │   │   └── validators.rs  # 内置校验器
│   │   └── Cargo.toml
│   │
│   ├── adapter/               # Adapter 层（新增）
│   │   ├── src/
│   │   │   ├── task.rs        # TaskAdapter
│   │   │   ├── quality.rs     # QualityAdapter
│   │   │   ├── knowledge.rs   # KnowledgeAdapter
│   │   │   ├── tool.rs        # ToolAdapter
│   │   │   └── lib.rs
│   │   └── Cargo.toml         # 依赖 DevMan crates
│   │
│   ├── repl/                  # REPL 交互模式
│   ├── daemon/                # 后台守护进程
│   ├── cli/                   # CLI 工具
│   └── observability/         # 可观测性层
│
└── docs/
    └── plans/
        └── 2026-02-04-devman-integration-design.md
```

---

## 8. 实现计划

### Phase 1: 最小可运行核心

1. **配置 DevMan 依赖**
   ```toml
   [workspace.dependencies]
   devman-core = { git = "https://github.com/Genuineh/DevMan" }
   devman-storage = { git = "https://github.com/Genuineh/DevMan" }
   devman-quality = { git = "https://github.com/Genuineh/DevMan" }
   devman-knowledge = { git = "https://github.com/Genuineh/DevMan" }
   devman-tools = { git = "https://github.com/Genuineh/DevMan" }
   ```

2. **实现 Decision Engine**
   - DecisionEngine trait
   - 基础校验器

3. **实现 Adapter 层**
   - TaskAdapter（NDC Intent → DevMan Task）
   - QualityAdapter

4. **基础 CLI**
   - `ndc create <task>`
   - `ndc status`
   - `ndc list`

### Phase 2: 完整适配

1. KnowledgeAdapter
2. ToolAdapter
3. AsyncTaskAdapter

### Phase 3: 交互层

1. REPL 模式
2. 守护进程模式

### Phase 4: 提交 DevMan 需求

1. 向量检索 Feature Request
2. 访问控制 Feature Request
3. 记忆稳定性 Feature Request

---

## 9. 设计原则总结

1. **架构独立**：NDC 有自己的领域模型，不依赖 DevMan 的类型
2. **决策优先**：所有操作必须先通过 Decision Engine 裁决
3. **适配转换**：Adapter 层负责 NDC ↔ DevMan 的双向转换
4. **贡献上游**：通用功能（向量、权限）提给 DevMan，社区共享

---

**文档版本**: 1.0
**最后更新**: 2026-02-04
