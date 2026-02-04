# NDC 最终设计方案

**项目名称**: NDC (Neo Development Companion)
**版本**: v1.0
**日期**: 2026-02-04
**状态**: 设计阶段

---

## 1. 项目概述

### 1.1 核心理念

NDC 是一个生产级 AI 工程体系，内置决策约束、任务管理、上下文管理，支持 REPL 对话和后台守护两种交互模式。

**四大核心理念**：

1. **有效且稳定的记忆（上下文）**
   - 通过认知网络等架构和算法方式来高效检索
   - 脱离生成式、脱离泛化，准确逻辑及边界下的稳定输出
   - AI 是大脑，它需要学会使用记忆来帮助自己更高效地完成任务

2. **稳定的工作机制**
   - 通过严格的流程校验代替泛化的 prompt 指导
   - AI 向系统申请任务，获取任务状态和执行指引
   - 任何 Agent 的输出都只是"提案"，不是"事实"

3. **严格的检验**
   - 强制测试验证，内置质量门禁
   - Hook 系统允许用户自定义验证策略

4. **与人的交互**
   - 人只负责思考和决策
   - 线性工作由 AI 结构化完成
   - 人类能够丝滑地观察 AI 的工作并交互

### 1.2 技术选型

| 功能 | 实现方式 |
|------|---------|
| **决策引擎** | NDC 独有实现 |
| **任务管理** | DevMan (git 依赖) |
| **质量引擎** | DevMan |
| **工具执行** | DevMan |
| **知识服务** | DevMan + 扩展规划 |
| **存储后端** | DevMan JsonStorage |
| **向量检索** | 提需求给 DevMan (#3) |
| **访问控制** | 提需求给 DevMan (#2) |
| **记忆稳定性** | 提需求给 DevMan (#1) |

### 1.3 参考项目

- **DevMan**: https://github.com/Genuineh/DevMan - AI 认知工作管理系统（执行后端）
- **Claude Code**: REPL 交互模式参考
- **OpenCode/Copilot CLI**: CLI 工具交互参考

---

## 2. 系统架构

### 2.1 架构分层

```
┌─────────────────────────────────────────────────────────────────────┐
│                        NDC 架构（最终版）                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    可观测性层                                 │   │
│  │  • Task Timeline  • Agent 行为日志  • 记忆访问轨迹           │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              ↓↑                                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │  ┌───────────────────────────────────────────────────────┐   │   │
│  │  │         Decision Engine (NDC 独有核心)                 │   │   │
│  │  │  • Intent 评估  • Verdict 裁决  • 约束校验            │   │   │
│  │  └───────────────────────────┬───────────────────────────┘   │   │
│  │                            │                                  │   │
│  │  ┌───────────────────────────▼───────────────────────────┐   │   │
│  │  │              NDC 领域层                                 │   │   │
│  │  │  • Intent/Verdict/Effect  • Agent/AgentRole           │   │   │
│  │  │  • MemoryStability  • Permission                      │   │   │
│  │  └───────────────────────────┬───────────────────────────┘   │   │
│  │                            │                                  │   │
│  │  ┌───────────────────────────▼───────────────────────────┐   │   │
│  │  │              Adapter 层（领域转换）                      │   │   │
│  │  │  • NDC Intent → DevMan Task  • NDC Memory → Knowledge │   │   │
│  │  └───────────────────────────┬───────────────────────────┘   │   │
│  │                            │                                  │   │
│  └────────────────────────────┼──────────────────────────────────┘   │
│                               │                                      │
│  ┌────────────────────────────▼──────────────────────────────────┐   │
│  │                    DevMan（执行后端）                           │   │
│  │  ✅ Task  ✅ Quality  ✅ Tools  ✅ Knowledge  ✅ Storage      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              ↓↑                                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    交互层                                     │   │
│  │  ┌─────────────┐  ┌─────────────────────────────────────┐   │   │
│  │  │ REPL 模式   │  │ 后台守护模式                        │   │   │
│  │  │ (对话式)    │  │ • gRPC 通信  • CLI 控制             │   │   │
│  │  └─────────────┘  └─────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              ↓↑                                    │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    插件层                                     │   │
│  │  • MCP 协议  • Skills 系统  • WASM 沙箱                      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 核心原则

1. **任何 Agent 的输出，都只是"提案"，不是"事实"**
2. **决策引擎是同步阻塞的**：没有 verdict，任何动作不能 commit
3. **NDC 拥有独立的领域模型**：不依赖 DevMan 的类型
4. **Adapter 负责双向转换**：NDC 模型 ↔ DevMan 模型
5. **插件永远不能绕过核心层**
6. **记忆写入 != 记忆使用**：强制分离

---

## 3. NDC 核心层

### 3.1 Decision Engine（NDC 独有）

**职责**：
- 行为合法性校验
- 任务边界检查
- 人类介入判断
- 继续/回退/停止 决策

**核心接口**：

```rust
#[async_trait]
pub trait DecisionEngine: Send + Sync {
    /// 评估 Intent 并返回 Verdict (同步阻塞)
    async fn evaluate(&self, intent: Intent) -> Verdict;

    /// 批量评估（用于并发场景）
    async fn evaluate_batch(&self, intents: Vec<Intent>) -> Vec<Verdict>;

    /// 注册校验器
    fn register_validator(&mut self, validator: Box<dyn Validator>);

    /// 获取当前策略状态
    fn policy_state(&self) -> PolicyState;
}
```

**Intent - AI 的意图提案**：

```rust
pub struct Intent {
    pub id: IntentId,
    pub agent: AgentId,
    pub agent_role: AgentRole,  // Planner/Implementer/Reviewer/Tester/Historian
    pub proposed_action: Action,
    pub effects: Vec<Effect>,   // 声明的影响范围
    pub reasoning: String,
    pub task_id: TaskId,
    pub timestamp: Timestamp,
}

/// Effect - 意图的影响范围
pub enum Effect {
    FileOperation { path: PathBuf, op: FileOp },
    TaskTransition { from: TaskState, to: TaskState },
    MemoryOperation { memory_id: MemoryId, op: MemoryOp },
    ToolInvocation { tool: String, args: Vec<String> },
    HumanInteraction { interaction_type: InteractionType },
}
```

**Verdict - 系统的裁决**：

```rust
pub enum Verdict {
    /// 允许执行
    Allow,
    /// 拒绝执行
    Deny { reason: String, code: ErrorCode },
    /// 需要人类介入
    RequireHuman {
        question: String,
        context: HumanContext,
        timeout: Option<Duration>,
    },
    /// 修改后执行
    Modify {
        original_action: Action,
        modified_action: Action,
        reason: String,
        warnings: Vec<String>,
    },
    /// 延迟决策（需要更多信息）
    Defer {
        required_info: Vec<InformationRequirement>,
        retry_after: Option<Duration>,
    },
}
```

**内置校验器**：

| 校验器 | 优先级 | 职责 |
|--------|--------|------|
| TaskBoundaryValidator | 100 | 确保 Action 不超出 Task 范围 |
| PermissionValidator | 90 | 确保 Agent 有执行权限 |
| MemoryAccessValidator | 80 | 确保 Memory 操作符合访问控制 |
| SecurityPolicyValidator | 70 | 防止危险操作 |
| DependencyValidator | 60 | 确保前置条件满足 |

### 3.2 NDC 领域模型

| 类型 | 说明 | DevMan 对应 |
|------|------|------------|
| Intent | AI 的意图提案 | 无（NDC 独有） |
| Verdict | 系统裁决 | 无（NDC 独有） |
| AgentRole | Planner/Implementer/Reviewer/Tester/Historian | 无（NDC 独有） |
| MemoryStability | Ephemeral/Derived/Verified/Canonical | 无（规划中，#1） |
| Permission | 基于 AgentRole 的权限 | 无（规划中，#2） |

### 3.3 Agent 角色模型

| 角色 | 职责 | 权限 |
|------|------|------|
| Planner | 规划任务、分解工作 | 创建任务、更新计划 |
| Implementer | 实现代码、执行操作 | 写代码、读记忆 |
| Reviewer | 审查代码、验证质量 | 读代码、标记完成 |
| Tester | 运行测试、验证结果 | 执行测试、写测试结果 |
| Historian | 记录历史、管理记忆 | 写记忆、更新记录 |

---

## 4. Adapter 层

### 4.1 设计理念

Adapter 层负责 NDC 领域模型与 DevMan 执行后端之间的双向转换：

1. **NDC → DevMan**：Intent 转换为 DevMan Task 调用
2. **DevMan → NDC**：DevMan 结果转换为 NDC 响应
3. **决策拦截**：所有调用先经过 Decision Engine

### 4.2 任务适配器

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

### 4.3 领域映射

| NDC 概念 | DevMan 概念 | 转换逻辑 |
|----------|-------------|---------|
| Intent | Task + WorkRecord | Intent 提案 → DevMan Task 创建 |
| AgentRole | Task assignee | Planner → DevMan Task 分配 |
| MemoryEntry | Knowledge | MemoryEntry → DevMan Knowledge |
| QualityCheck | devman_run_quality_check | 直接映射 |
| ToolExecution | devman_execute_tool | 直接映射 |

### 4.4 状态映射

| NDC TaskState | DevMan TaskState | 说明 |
|---------------|-----------------|------|
| Pending | Created | 初始状态 |
| Preparing | ContextRead | 准备中 |
| InProgress | InProgress | 执行中 |
| AwaitingVerification | QualityChecking | 等待质检 |
| Completed | Completed | 已完成 |
| Failed | Abandoned | 失败/放弃 |

---

## 5. DevMan 后端

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

### 5.2 依赖配置

```toml
[workspace.dependencies]
# DevMan - AI 工作管理系统 (git dependency)
devman-core = { git = "https://github.com/Genuineh/DevMan" }
devman-storage = { git = "https://github.com/Genuineh/DevMan" }
devman-quality = { git = "https://github.com/Genuineh/DevMan" }
devman-knowledge = { git = "https://github.com/Genuineh/DevMan" }
devman-tools = { git = "https://github.com/Genuineh/DevMan" }
```

---

## 6. 待提交给 DevMan 的功能需求

以下 Feature Request 已提交到 DevMan 项目：

| # | 功能 | Issue | 优先级 |
|---|------|-------|--------|
| 1 | 知识稳定性等级 | [#1](https://github.com/Genuineh/DevMan/issues/1) | 中 |
| 2 | 访问控制 | [#2](https://github.com/Genuineh/DevMan/issues/2) | 中 |
| 3 | 向量检索 | [#3](https://github.com/Genuineh/DevMan/issues/3) | 高 |

详细内容见：`docs/devman-feature-requests.md`

---

## 7. 可观测性层

**一等展示对象**：

1. **Task Timeline**：完整任务时间线
2. **Agent 行为日志**：所有 Agent 操作轨迹
3. **记忆访问轨迹**：谁在何时访问了什么上下文
4. **测试结果历史**：质量趋势分析

**人类控制面板**：

- 当前 Task 在哪个 State
- 为什么停在这里（哪条规则 / 哪个测试）
- 如果我介入，我是在"决定什么"

---

## 8. 交互层

### 8.1 REPL 模式

持续对话式交互，类似 Claude Code。

```
$ ndc repl
NDC v1.0 - Type 'help' for commands

ndc> I want to add authentication to my API

[Planner] Creating task: Add JWT authentication
[Decision] Task created: #1234
ndc> Please implement it

[Implementer] Reading API structure...
[Decision] Allow: Read API documentation
[Implementer] Writing auth module...
[Decision] Allow: Create new file
...
[Decision] RequireHuman: Which JWT library do you prefer?
ndc> Use jsonwebtoken

[Implementer] Continuing implementation...
ndc>
```

### 8.2 后台守护模式

长期运行的守护进程 + CLI 控制。

```bash
# 启动守护进程
$ ndc daemon start

# 在另一个终端控制
$ ndc send-task "Add authentication"
Task #1234 created

$ ndc status
Task #1234: InProgress
- Agent: Implementer
- Current action: Writing auth module

$ ndc logs --task 1234
[10:30:15] [Planner] Created task
[10:30:20] [Implementer] Reading API structure
[10:30:25] [Decision] Allow: Read API documentation
...
```

**通信方式**：gRPC（支持流式传输）

---

## 9. 插件层

### 9.1 插件接口哲学

**插件可以**：
- 申请任务
- 请求能力
- 返回结果

**插件不能**：
- 直接改状态
- 决定流程
- 写核心记忆

### 9.2 支持的插件类型

| 类型 | 协议 | 用途 |
|------|------|------|
| MCP | Model Context Protocol | 与 MCP 服务器集成 |
| Skills | 内置系统 | 工作流模板、最佳实践 |
| WASM | WebAssembly | 沙箱化自定义扩展 |

---

## 10. 项目结构

```
ndc/
├── Cargo.toml                 # Workspace 配置（含 DevMan 依赖）
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
│   ├── adapter/               # Adapter 层（领域转换）
│   │   ├── src/
│   │   │   ├── task.rs        # TaskAdapter
│   │   │   ├── quality.rs     # QualityAdapter
│   │   │   ├── knowledge.rs   # KnowledgeAdapter
│   │   │   ├── tool.rs        # ToolAdapter
│   │   │   └── lib.rs
│   │   └── Cargo.toml         # 依赖 DevMan crates
│   │
│   ├── observability/         # 可观测性层
│   │   ├── src/
│   │   │   ├── timeline.rs    # Task Timeline
│   │   │   └── logs.rs        # Agent 行为日志
│   │   └── Cargo.toml
│   │
│   ├── repl/                  # REPL 交互模式
│   ├── daemon/                # 后台守护进程
│   ├── cli/                   # CLI 工具
│   └── plugins/               # 插件系统
│
├── docs/
│   ├── plans/
│   │   └── 2026-02-04-ndc-final-design.md    # 本文档
│   └── devman-feature-requests.md             # DevMan 需求
│
└── README.md
```

---

## 11. 实现计划

### Phase 1: 最小可运行核心 (MRC)

1. **配置 DevMan 依赖**
   - 添加 git 依赖到 workspace
   - 验证编译通过

2. **实现核心数据模型** (`crates/core`)
   - Intent, Verdict, Effect
   - Agent, AgentRole, Permission
   - MemoryEntry, MemoryStability

3. **实现 Decision Engine** (`crates/decision`)
   - DecisionEngine trait
   - 基础校验器

4. **实现 Adapter 层** (`crates/adapter`)
   - TaskAdapter（NDC Intent → DevMan Task）
   - QualityAdapter

5. **基础 CLI** (`crates/cli`)
   - `ndc create <task>`
   - `ndc status`
   - `ndc list`

### Phase 2: 完整适配

1. **KnowledgeAdapter**
   - NDC Memory → DevMan Knowledge
   - 稳定性等级支持

2. **ToolAdapter**
   - NDC Action → DevMan Tool

3. **AsyncTaskAdapter**
   - JobManager 集成

### Phase 3: 交互层

1. **REPL 模式** (`crates/repl`)
   - 对话式交互
   - Decision 结果展示

2. **守护进程模式** (`crates/daemon`)
   - gRPC 服务器
   - CLI 控制

### Phase 4: 可观测性

1. **Task Timeline**
2. **Agent 行为日志**
3. **记忆访问轨迹**

### Phase 5: 生产就绪

1. 完整测试覆盖
2. 性能优化
3. 文档完善
4. 安全审计

---

## 12. 设计原则总结

1. **AI 没有自由行动权** - 所有行为需有合法性来源
2. **智能与裁决权分离** - AI = 执行与推理，Decision & Policy = 允许/不允许
3. **架构独立** - NDC 有自己的领域模型，不依赖 DevMan 的类型
4. **决策优先** - 所有操作必须先通过 Decision Engine 裁决
5. **适配转换** - Adapter 层负责 NDC ↔ DevMan 的双向转换
6. **记忆是资产不是副产品** - 严格的写入控制和稳定性等级
7. **可组合的验证规则** - 灵活且可演进
8. **可观测性是一等能力** - 信任的基础
9. **人机协作制度化** - 明确人类介入的时机和方式
10. **贡献上游** - 通用功能（向量、权限）提给 DevMan，社区共享

---

## 13. 附录

### 13.1 相关文档

- [DevMan 项目](https://github.com/Genuineh/DevMan)
- [DevMan MCP API](https://github.com/Genuineh/DevMan/blob/main/docs/MCP_API.md)
- [Feature Request #1: 知识稳定性](https://github.com/Genuineh/DevMan/issues/1)
- [Feature Request #2: 访问控制](https://github.com/Genuineh/DevMan/issues/2)
- [Feature Request #3: 向量检索](https://github.com/Genuineh/DevMan/issues/3)

### 13.2 Task 生命周期时序图

```
┌─────────┐     ┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│  Agent  │     │   Adapter    │     │ DecisionEngine  │     │   DevMan     │
└────┬────┘     └──────┬───────┘     └────────┬────────┘     └──────┬───────┘
     │                  │                      │                      │
     │ submit(Intent)   │                      │                      │
     │─────────────────>│                      │                      │
     │                  │ evaluate(Intent)     │                      │
     │                  │─────────────────────>│                      │
     │                  │                      │                      │
     │                  │ Verdict::Allow       │                      │
     │                  │<─────────────────────│                      │
     │                  │                      │                      │
     │                  │ create_devman_task() │                      │
     │                  │─────────────────────────────────────────────>│
     │                  │                      │                      │
     │                  │ TaskId               │                      │
     │                  │<─────────────────────────────────────────────│
     │                  │                      │                      │
     │<─────────────────│ TaskId               │                      │
     │                  │                      │                      │
```

---

**文档版本**: 1.0 (最终版)
**最后更新**: 2026-02-04
**状态**: 设计完成，等待实现
