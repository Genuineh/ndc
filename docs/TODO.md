# NDC 实现待办清单

> **重要更新 (2026-02-06)**: LLM 集成 - 纯 LLM + 强制工程约束

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 (Task-Intent 合一) ✅ 已完成
├── decision/          # [大脑] 决策引擎 ✅ 已完成
├── runtime/           # [身体] 执行与验证 (Tools + Quality) ✅ 已完成
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon) ✅ 已完成
```

## 已完成模块 ✅

| 模块 | 文件 | 状态 | 说明 |
|------|------|------|------|
| **core** | task.rs | ✅ | Task, TaskState, ExecutionStep, ActionResult |
| **core** | intent.rs | ✅ | Intent, Verdict, PrivilegeLevel, Effect |
| **core** | agent.rs | ✅ | AgentRole, AgentId, Permission |
| **core** | memory.rs | ✅ | MemoryStability, MemoryQuery, MemoryEntry |
| **decision** | engine.rs | ✅ | DecisionEngine, validators |
| **runtime** | executor.rs | ✅ | Task execution, tool coordination |
| **runtime** | workflow.rs | ✅ | State machine, transitions |
| **runtime** | storage.rs | ✅ | In-memory storage |
| **runtime** | storage_sqlite.rs | ✅ | SQLite storage (6 tests) |
| **core** | lib.rs | ✅ | 37 unit tests |
| **decision** | lib.rs | ✅ | 21 integration tests |
| **runtime** | tools/mod.rs | ✅ | Tool, ToolManager |
| **runtime** | tools/fs.rs | ✅ | File operations |
| **runtime** | tools/git.rs | ✅ | Git operations (shell-based) |
| **runtime** | tools/shell.rs | ✅ | Shell command execution |
| **runtime** | verify/mod.rs | ✅ | QualityGateRunner |
| **interface** | cli.rs | ✅ | CLI commands (11 tests) |
| **interface** | daemon.rs | ✅ | gRPC service framework |
| **interface** | grpc.rs | ✅ | gRPC service impl (12 tests) |
| **interface** | repl.rs | ✅ | REPL mode - LLM-powered intent parsing (15 tests) |
| **interface** | e2e_tests.rs | ✅ | E2E tests (17 tests) |
| **interface** | grpc_client.rs | ✅ | gRPC client SDK (10 tests) |
| **core** | llm/mod.rs | ⏳ | LLM Provider 接口 (规划中) |
| **core** | llm/openai.rs | ⏳ | OpenAI Provider (规划中) |
| **core** | llm/anthropic.rs | ⏳ | Anthropic Provider (规划中) |
| **core** | llm/minimax.rs | ⏳ | MiniMax Provider (规划中) |
| **core** | llm/intent.rs | ⏳ | LLM Intent Parser (规划中) |

---

## 当前状态

### ✅ ndc-core (核心)

```
- Task / TaskId / TaskState
- Intent / Verdict / Action / Effect
- AgentRole / AgentId / Permission
- Memory / MemoryId / MemoryStability
- PrivilegeLevel (Normal/Elevated/High/Critical)
- QualityGate / QualityCheck / GateStrategy
```

### ✅ ndc-decision (决策)

```
- DecisionEngine
- Intent evaluation
- Privilege checking
- Condition validation
```

### ✅ ndc-runtime (执行)

```
- Executor: 任务创建和执行
- WorkflowEngine: 状态机转换
- Storage: 内存存储
- Tools:
  - FsTool: read/write/create/delete/list
  - GitTool: status/branch/commit/log/stash (shell-based)
  - ShellTool: whitelisted commands
- QualityGateRunner: tests/lint/typecheck/build
```

### ✅ ndc-interface (交互)

```
CLI Commands:
- create - 创建任务
- list - 列出任务
- status - 查看状态
- logs - 查看日志
- run - 执行任务
- rollback - 回滚任务
- repl - 启动 REPL
- daemon - 启动守护进程
- search - 搜索记忆

gRPC Services (with --features grpc):
- HealthCheck - 健康检查
- CreateTask - 创建任务
- GetTask - 获取任务
- ListTasks - 列出任务
- ExecuteTask - 执行任务
- RollbackTask - 回滚任务
- GetSystemStatus - 系统状态

gRPC Client SDK (with --features grpc):
- NdcClient - 客户端实例
- ClientConfig - 客户端配置
- create_client() - 便捷连接函数
- Connection pooling - 连接池管理
- Retry with exponential backoff - 指数退避重试
```

---

## 待实现功能 📋

### 1. 持久化存储

```
当前状态：SQLite 存储已完成 ✅
需要实现：
- [x] SQLite 存储 (crates/runtime/src/storage_sqlite.rs)
- [x] 6 个 SQLite 单元测试
- [ ] 存储迁移
```

### 2. REPL 增强 ✅

```
当前状态：REPL 增强已完成
已实现：
- [x] 完整意图解析 (LLM-powered)
- [x] 任务自动创建 (从对话自动创建任务)
- [x] 上下文保持 (会话状态、对话历史、实体提取)
- [x] 15 个 REPL 单元测试
```

### 3. 测试覆盖 ✅

```
当前状态：150 个测试全部通过
已实现：
- [x] Core 单元测试 (37 tests) ✅
- [x] Decision 集成测试 (21 tests) ✅
- [x] Runtime 工具测试 (37 tests) ✅
- [x] E2E 测试 (17 tests) ✅
- [x] CLI 测试 (11 tests) ✅
- [x] gRPC/Daemon 测试 (6 tests) ✅
- [x] REPL 测试 (15 tests) ✅
- [x] SQLite 测试 (6 tests) ✅
```

### 4. gRPC 客户端库 ✅

```
当前状态：客户端库已完成
已实现：
- [x] 客户端 SDK (NdcClient, ClientConfig)
- [x] 连接池 (PooledChannel, pool management)
- [x] 重试机制 (exponential backoff, retry logic)
- [x] 10 个 gRPC 客户端单元测试
```

### 5. LLM 集成 - 强制工程约束 ⏳

```
核心理念：LLM + 强制工程约束 = 稳定高质量代码

📄 详细设计: docs/ENGINEERING_CONSTRAINTS.md

组件整合:
- Task 状态机: Pending → Preparing → InProgress → AwaitingVerification → Completed
- Memory 稳定性: Ephemeral → Derived → Verified → Canonical
- 质量门禁: Test → Lint → TypeCheck → Build

工程约束流程:
  用户需求 ──▶ LLM 分解 ──▶ 结构校验 ──▶ 执行 ──▶ 验证 ──▶ 完成
                    │           │           │        │
                    ▼           ▼           ▼        ▼
                 不通过?      不通过?     不通过?   不通过?
                    │           │           │        │
                    └───────────┴───────────┴────────┘
                               │
                         强制重来 N 次
                               │
                    ┌──────────▼──────────┐
                    │  超过次数?          │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │  需要人工介入        │
                    └────────────────────┘
```

#### 核心组件 ⏳

```
crates/core/src/
├── llm/
│   ├── decomposer.rs       # Preparing: 任务分解器 ⏳
│   ├── validator.rs        # Preparing: 计划校验器 ⏳
│   └── retry.rs           # 全局: 强制重来引擎 ⏳
├── task/
│   └── state_machine.rs    # 状态机扩展 ⏳
└── memory/
    └── stability.rs        # 稳定性升级 ⏳

crates/runtime/src/
├── executor/
│   ├── step_engine.rs     # InProgress: 步骤执行引擎 ⏳
│   └── quality_gate.rs     # InProgress: 质量门禁 ⏳
└── verification/
    └── verifier.rs         # AwaitingVerification: 验收 ⏳
```

#### 实现步骤

##### 5.1 配置系统 ✅
- [x] 配置文件格式设计 (YAML)
- [x] 环境变量支持
- [x] 多 Provider 配置（OpenAI/Anthropic/MiniMax）
- [x] 重试/分解/验收配置

##### 5.2 LLM Provider 接口 ⏳
- [ ] LlmProvider trait 定义
- [ ] LlmMessage / LlmResponse 类型
- [ ] 流式输出支持
- [ ] Provider 实现：
  - [ ] OpenAI Provider (GPT-4o)
  - [ ] Anthropic Provider (Claude 3.5)
  - [ ] MiniMax Provider (MiniMax API)

##### 5.3 Task Decomposer ⏳ (Preparing 阶段)
```
职责:
- LLM 分解用户需求为 TaskPlan
- 强制校验: 完整性/依赖/知识库
- Memory: Ephemeral → Derived

强制约束:
├── 必须返回结构化 TaskPlan
├── 每个 step 必须有: title, description, input, output, validation
│   └── 校验不通过 → 重来 N 次 → 人工介入
│   ├── 不能为空分解
│   └── 不能漏掉关键步骤
├── 校验器:
│   ├── completeness_check - 完整性检查
│   ├── dependency_check - 依赖关系检查
│   └── validation_check - 验收标准检查
└── 输出: ValidatedTaskPlan
```

- [ ] TaskPlan 结构体定义
- [ ] TaskStep 结构体定义
- [ ] DecomposeEngine - 分解引擎
- [ ] PlanValidator - 计划校验器（强制约束）
- [ ] RetryPolicy - 重试策略配置
- [ ] HumanInterventionHandler - 人工介入处理器

##### 5.4 REPL Intent Parser ⏳
- [ ] LLM IntentParser 实现（纯 LLM，无正则）
- [ ] 上下文保持
- [ ] 实体提取
- [ ] 置信度计算

##### 5.5 质量门禁 ⏳
- [ ] QualityGate 集成
- [ ] 编译检查 (cargo check)
- [ ] 测试执行 (cargo test)
- [ ] Lint 检查 (cargo clippy)
- [ ] 门禁失败 → 重来

##### 5.6 强制重来引擎 ⏳
```
RetryEngine 配置:
├── max_retries: 3           // 最大重试次数
├── retry_delay: 1000        // 重试延迟(ms)
├── backoff_multiplier: 2     // 指数退避
├── max_delay: 30000          // 最大延迟(ms)
└── human_intervention_after: 3  // 人工介入阈值
```

- [ ] RetryPolicy 结构体
- [ ] RetryEngine 实现
- [ ] 自动重试逻辑
- [ ] 人工介入触发

##### 5.7 状态报告 ⏳
- [ ] ExecutionState 状态跟踪
- [ ] ProgressReport 进度报告
- [ ] FailureReport 失败报告（含改进建议）
- [ ] HumanInterventionRequest 人工请求

#### 代码结构

```
crates/core/src/llm/
├── mod.rs                    # 模块入口
├── provider/
│   ├── mod.rs              # Provider trait
│   ├── openai.rs           # OpenAI 实现
│   ├── anthropic.rs        # Anthropic 实现
│   └── minimax.rs          # MiniMax 实现
├── decomposer/
│   ├── mod.rs              # 分解器模块
│   ├── task_plan.rs        # TaskPlan 结构
│   ├── validator.rs        # 计划校验器
│   └── engine.rs           # 分解引擎
├── parser/
│   ├── mod.rs              # Intent Parser
│   └── intent.rs           # 意图解析
└── retry/
    ├── mod.rs              # 重试模块
    ├── engine.rs           # 重试引擎
    └── policy.rs           # 重试策略
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

# 启用 gRPC
cargo build --features grpc

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

1. **LLM Provider** - OpenAI/Anthropic/MiniMax 实现
2. **Task Decomposer** - 强制分解约束引擎
3. **Retry Engine** - 强制重来机制
4. **Human Intervention** - 人工介入处理

---

最后更新: 2026-02-06 (LLM 集成 - 纯 LLM + 强制工程约束)
标签: #ndc #llm #engineering-constraints
