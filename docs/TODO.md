# NDC 实现待办清单

> **重要更新 (2026-02-06)**: CLI + gRPC 服务 + REPL 增强完成，96 个测试通过

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
| **interface** | repl.rs | ✅ | REPL mode (15 intent parsing tests) |

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
- [x] 完整意图解析 (正则表达式模式匹配)
- [x] 任务自动创建 (从对话自动创建任务)
- [x] 上下文保持 (会话状态、对话历史、实体提取)
- [x] 15 个 REPL 单元测试
```

### 3. 测试覆盖 ✅

```
当前状态：96 个测试
已实现：
- [x] Core 单元测试 (37 tests) ✅
- [x] Decision 集成测试 (21 tests) ✅
- [x] REPL 测试 (15 tests) ✅
- [x] SQLite 测试 (6 tests) ✅
- [x] CLI 测试 (11 tests) ✅
- [x] gRPC/Daemon 测试 (6 tests) ✅
待实现：
- [ ] 工具测试 (fs/git/shell)
- [ ] E2E 测试 (CLI commands)
```

### 4. gRPC 客户端库

```
当前状态：服务端完成
需要实现：
- [ ] 客户端 SDK
- [ ] 连接池
- [ ] 重试机制
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

1. **持久化存储** - JSON 文件后端
2. **工具测试** - fs/git/shell 工具测试
3. **E2E 测试** - CLI 命令端到端测试
4. **gRPC 客户端库** - 提供客户端 SDK

---

最后更新: 2026-02-06 (测试覆盖完成 - 96 tests)
标签: #ndc #todo
