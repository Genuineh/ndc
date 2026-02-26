# NDC 文档导航

> **最后更新**: 2026-02-25

本文档整合了 NDC 项目的所有文档，提供清晰的导航结构。

当前执行状态：

- `P0-D`（安全边界与项目级会话隔离）进行中
- `P1`（核心自治能力与治理）为下一优先级

## 快速链接

- **[USER_GUIDE.md](./USER_GUIDE.md) - 用户使用指南**
- **[GRPC_CLIENT.md](./GRPC_CLIENT.md) - gRPC 客户端指南**
- **[LLM_INTEGRATION.md](./LLM_INTEGRATION.md) - LLM 提供商集成方案**
- **[ENGINEERING_CONSTRAINTS.md](./ENGINEERING_CONSTRAINTS.md) - 工程约束与阶段设计**
- **[plan/current_plan.md](./plan/current_plan.md) - 当前执行计划**
- **[plan/archive/NDC_AGENT_INTEGRATION_PLAN.md](./plan/archive/NDC_AGENT_INTEGRATION_PLAN.md) - 历史计划（归档）**

## 项目概述

NDC (Neo Development Companion) 是一个面向 coding agent 的工程框架，采用自然语言交互 + 工具驱动执行模式。

### 当前能力

- **自然语言交互主链**: `run/repl` 直接调用 Agent
- **多 Provider LLM 支持**: OpenAI/Anthropic/Ollama/MiniMax/OpenRouter
- **默认工具生态**: 文件、搜索、Shell、Git、Web 工具统一注册
- **验证与工作流基础**: TaskVerifier、Workflow/Saga、QualityGate 基础能力
- **扩展接口**: MCP/Skills 结构已具备，持续接入中

### 文档结构

```
docs/
├── README.md           # 本文档（导航页）
├── TODO.md             # 代办清单（只维护待办）
├── plan/
│   ├── current_plan.md # 当前执行计划（唯一）
│   └── archive/        # 已完成计划归档
├── USER_GUIDE.md       # 用户使用指南（必读）
├── GRPC_CLIENT.md      # gRPC 客户端指南
└── ...
```

## 模块说明

| 模块 | 位置 | 说明 |
|------|------|------|
| ai_agent | crates/core/src/ai_agent/ | Agent Orchestrator, Session, Verifier, Prompts |
| core/task | crates/core/src/task.rs | Task, TaskState, ExecutionStep |
| core/llm | crates/core/src/llm/ | LLM Provider 系统 (OpenAI/Anthropic/Ollama/MiniMax/OpenRouter) |
| storage | crates/storage/src/ | 存储抽象层 (Storage trait, MemoryStorage, SqliteStorage) |
| runtime/tools | crates/runtime/src/tools/ | MCP, Skills, File, Git, Shell |
| interface | crates/interface/src/ | CLI, REPL, gRPC, Interactive |
| decision | crates/decision/src/ | 决策引擎 |
| bin | bin/main.rs | 二进制入口 |

## 快速开始

```bash
# 构建项目
cargo build

# 运行测试
cargo test -q

# 查看帮助
cargo run -- --help
```

## 配置系统 (OpenCode 风格)

NDC 采用 OpenCode 风格的分层配置系统，支持配置文件和环境变量。

### 配置分层

| 层级 | 路径 | 说明 |
|------|------|------|
| 全局 | `/etc/ndc/config.yaml` | 系统级配置 |
| 用户 | `~/.config/ndc/config.yaml` | 用户级配置 |
| 项目 | `./.ndc/config.yaml` | 项目级配置 |

优先级：项目 > 用户 > 全局

### 环境变量

| 变量 | 说明 | 默认值 |
|------|------|-------|
| `NDC_LLM_PROVIDER` | LLM 提供商 | openai |
| `NDC_LLM_MODEL` | 模型名称 | gpt-4o |
| `NDC_LLM_API_KEY` | API Key | - |
| `NDC_LLM_BASE_URL` | API Base URL | - |
| `NDC_ORGANIZATION` | 组织 ID | - |
| `NDC_REPL_CONFIRMATION` | 确认模式 | true |
| `NDC_MAX_CONCURRENT_TASKS` | 最大并发数 | 4 |

### 配置文件示例

```yaml
# ~/.config/ndc/config.yaml

llm:
  enabled: true
  provider: openai
  model: gpt-4o
  api_key: env://OPENAI_API_KEY  # 或直接填写
  base_url: https://api.openai.com/v1
  temperature: 0.1
  max_tokens: 4096

repl:
  prompt: "ndc> "
  show_thought: true
  confirmation_mode: true

runtime:
  max_concurrent_tasks: 4
  execution_timeout: 300

agents:
  - name: default
    provider: openai
    model: gpt-4o
    task_types:
      - "*"

  - name: implementer
    provider: anthropic
    model: claude-sonnet-4-5-20250929
    task_types:
      - implementation
      - bugfix
```

## 开发指南

### 代码规范

1. **错误处理**: 使用 `Result<T>` 和 `?` 操作符
2. **异步设计**: 使用 `async fn` 和 `.await`
3. **日志记录**: 使用 `tracing::info/warn/error`
4. **配置管理**: 使用结构体和 `derive(Debug, Clone, Serialize, Deserialize)`
5. **测试编写**: 每个模块包含单元测试

### Git 工作流

```bash
# 功能开发
git checkout -b feature/<branch-name>
git commit -m "type(scope): message"

# 发布
git push origin main
```

---

> 提示：当前计划请阅读 [plan/current_plan.md](./plan/current_plan.md)，历史方案请查阅 [plan/archive/](./plan/archive/)。

---
