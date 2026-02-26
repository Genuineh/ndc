# NDC - Tool-Driven Coding Agent

NDC (Neo Development Companion) 是一个 Rust 实现的 coding agent 框架。  
目标是采用类似 OpenCode 的自然语言交互方式，同时引入自有工程工具体系（工具注册、验证、质量门禁、MCP/Skills 扩展）。

## 当前定位

- 交互入口以自然语言为主：`ndc run --message` 和 `ndc repl`
- Agent 使用默认工具集（文件、搜索、Shell、Git、Web）执行任务
- LLM 请求支持 function-calling 工具 schema 透传（OpenAI/OpenRouter 通道）
- 任务验证链（TaskVerifier）和质量门禁能力已集成在架构中

## 快速开始

```bash
# 构建
cargo build

# 查看命令
cargo run -- --help

# 一次性调用
cargo run -- run --message "请分析这个仓库并给出重构建议"

# 进入交互模式
cargo run -- repl
```

## 命令

| 命令 | 说明 |
|---|---|
| `ndc run --message "..."` | 单轮自然语言请求 |
| `ndc run` | 直接进入 REPL |
| `ndc repl` | 交互式 Agent 会话 |
| `ndc daemon` | 后台服务 |
| `ndc search <query>` | 记忆检索入口（持续完善中） |
| `ndc status-system` | 系统状态 |

## 架构概览

```text
bin (CLI entry)
  -> interface (CLI/REPL/daemon, agent mode)
     -> core (agent orchestrator, llm providers, verifier, config)
     -> runtime (tools, workflow, executor, mcp, skill)
        -> storage (trait-based storage abstraction)
     -> decision (decision engine)
```

核心目录：

- `crates/core`: Agent Orchestrator、LLM Provider、Prompt/Verifier、配置加载
- `crates/runtime`: Tool 系统、Workflow/Saga、Executor、MCP、Skills
- `crates/storage`: 存储抽象层（Storage trait、MemoryStorage、SqliteStorage）
- `crates/decision`: 决策引擎
- `crates/interface`: CLI/REPL/Daemon 与 Agent 交互层
- `docs`: 使用文档、演进计划、重构方案

## LLM Provider

支持 `openai` / `anthropic` / `minimax` / `openrouter` / `ollama`。  
环境变量采用 `NDC_` 前缀，例如：

```bash
export NDC_OPENAI_API_KEY="..."
export NDC_OPENROUTER_API_KEY="..."
export NDC_MINIMAX_API_KEY="..."
export NDC_MINIMAX_GROUP_ID="..."
```

也可通过 `~/.config/ndc/config.yaml` 或项目 `.ndc/config.yaml` 配置。

## 质量基线

当前主分支测试：

- `cargo test -q` 全量通过
- Rust edition 2024，全 crate 统一 `edition.workspace = true`

## 文档

- `docs/USER_GUIDE.md`
- `docs/LLM_INTEGRATION.md`
- `docs/GRPC_CLIENT.md`
- `docs/plan/current_plan.md`（当前执行计划）
- `docs/plan/archive/`（已归档计划）

## 许可证

MIT
