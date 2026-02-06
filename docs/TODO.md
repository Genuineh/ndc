# NDC 实现待办清单

> **重要更新 (2026-02-06)**: LLM 集成 - 知识驱动 + TODO 映射 + 完整工程流程

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 + LLM Provider + TODO 管理 ✅ 已完成
├── decision/          # [大脑] 决策引擎 ✅ 已完成
├── runtime/           # [身体] 执行与验证 + 工作流引擎 ⏳
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon) ✅ 已完成
```

## 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────┐
│              NDC 知识驱动开发流程                                    │
│                                                                     │
│  知识库 ──▶ 理解需求 ──▶ TODO 映射 ──▶ 分解 ──▶ 执行 ──▶ 验收   │
│                                                                     │
│  文档 ──▶ 更新 ──▶ 完成 ──▶ 通知用户                               │
│                                                                     │
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

## LLM 集成 - 知识驱动 + TODO 映射 ⏳

```
核心理念：知识驱动开发，TODO 映射，完整工程流程

📄 详细设计: docs/ENGINEERING_CONSTRAINTS.md

六大阶段:
1. 理解需求 → 检索知识库 + 检查 TODO
2. 建立映射 → 关联/创建总 TODO
3. 分解任务 → LLM 分解为原子子任务
4. 执行开发 → 质量门禁 + 重来机制
5. 验收确认 → 自动/人工验收
6. 更新文档 → 知识库 + 通知用户
```

### 核心组件 ⏳

```
crates/core/src/
├── llm/
│   ├── mod.rs              # Provider Trait + 接口 ⏳
│   ├── provider/
│   │   ├── mod.rs          # Trait 定义
│   │   ├── openai.rs       # OpenAI ⏳
│   │   ├── anthropic.rs     # Anthropic ⏳
│   │   └── minimax.rs      # MiniMax ⏳
│   ├── understanding.rs     # 阶段 1: 需求理解 ⏳
│   └── decomposition.rs    # 阶段 3: 任务分解 ⏳
│
├── todo/
│   ├── mod.rs              # TODO 管理模块 ⏳
│   ├── project_todo.rs     # 总 TODO 结构 ⏳
│   ├── task_chain.rs       # 子任务链 ⏳
│   └── mapping_service.rs   # 阶段 2: 映射服务 ⏳
│
└── memory/
    └── knowledge_base.rs     # 知识库管理 ⏳

crates/runtime/src/
├── engine/
│   ├── mod.rs              # 工作流引擎 ⏳
│   ├── workflow_engine.rs   # 完整流程控制 ⏳
│   ├── execution_engine.rs  # 阶段 4: 执行引擎 ⏳
│   └── acceptance_engine.rs # 阶段 5: 验收引擎 ⏳
│
└── documentation/
    └── updater.rs          # 阶段 6: 文档更新 ⏳
```

### 实现步骤

#### 阶段 1: 需求理解 ⏳

```
职责:
- 检索知识库文档
- 检查总 TODO 映射
- LLM 分析需求

输出: RequirementContext
```

- [ ] KnowledgeBase 检索接口
- [ ] TodoIndex 相似度搜索
- [ ] LLM 需求分析 Prompt
- [ ] UnderstandingResult 结构

#### 阶段 2: TODO 映射 ⏳

```
职责:
- 检查是否已有 TODO
- 创建/关联总 TODO
- 通知用户确认

输出: TodoMappingResult
```

- [ ] ProjectTodo 结构
- [ ] TodoState 状态机
- [ ] MappingService 实现
- [ ] NotificationService

#### 阶段 3: 任务分解 ⏳

```
职责:
- LLM 分解为子任务
- 创建 TaskChain
- 记录依赖关系

输出: TaskChain
```

- [ ] SubTask 结构
- [ ] TaskChain 结构
- [ ] DependencyGraph
- [ ] DecompositionService

#### 阶段 4: 执行开发 ⏳

```
职责:
- 执行子任务
- 质量门禁检查
- 强制重来机制
- 人工介入处理

子任务循环:
  开发 → 测试 → 质量门禁 → 验证 → 重来/下一步
```

- [ ] StepExecutionEngine
- [ ] QualityGateRunner 集成
- [ ] RetryEngine
- [ ] HumanInterventionHandler

#### 阶段 5: 验收确认 ⏳

```
职责:
- 自动验收检查
- 人工验收请求
- 验收结果记录

验收标准:
- 测试覆盖率 >= 80%
- 所有测试通过
- 编译无警告
```

- [ ] AcceptanceCriteria 结构
- [ ] AcceptanceService
- [ ] HumanReviewRequest

#### 阶段 6: 文档更新 ⏳

```
职责:
- 更新相关文档
- 记录决策变更
- 提升知识库稳定性
- 发送完成通知

输出: CompletionReport
```

- [ ] DocumentationService
- [ ] DocumentChanges 结构
- [ ] NotificationService
- [ ] KnowledgeBase 稳定性升级

### LLM Provider 实现

```
接口:
├── LlmProvider Trait
│   ├── chat() → LlmResponse
│   ├── chat_stream() → Stream
│   └── is_healthy() → bool
│
├── LlmMessage / LlmResponse
├── TokenUsage
└── LlmError
```

- [ ] OpenAI Provider (GPT-4o)
- [ ] Anthropic Provider (Claude 3.5)
- [ ] MiniMax Provider (MiniMax API)

### 代码结构

```
crates/core/src/llm/
├── mod.rs                    # 模块入口 + Trait
├── provider/
│   ├── mod.rs              # Trait 定义
│   ├── openai.rs           # OpenAI 实现
│   ├── anthropic.rs        # Anthropic 实现
│   └── minimax.rs          # MiniMax 实现
├── understanding/
│   ├── mod.rs              # 理解服务
│   └── analyzer.rs          # 需求分析
└── decomposition/
    ├── mod.rs              # 分解服务
    ├── planner.rs          # 任务规划
    └── validator.rs         # 分解校验

crates/core/src/todo/
├── mod.rs                    # 模块入口
├── project_todo.rs          # 总 TODO
├── subtask.rs               # 子任务
├── task_chain.rs            # 任务链
└── mapping.rs               # 映射服务

crates/runtime/src/engine/
├── mod.rs                    # 模块入口
├── workflow.rs              # 工作流引擎
├── executor.rs              # 执行引擎
└── acceptance.rs            # 验收引擎
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

# 列出任务
./target/debug/ndc list
```

---

## 下一步工作

1. **LLM Provider** - OpenAI/Anthropic/MiniMax 接口
2. **KnowledgeBase** - 文档检索和更新
3. **TODO 系统** - 映射和追踪
4. **Workflow Engine** - 完整流程编排
5. **Documentation** - 文档变更管理

---

最后更新: 2026-02-06 (LLM 集成 - 知识驱动 + TODO 映射)
标签: #ndc #llm #knowledge-driven #todo-mapping
