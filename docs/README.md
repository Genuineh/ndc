# NDC 文档导航

> **最后更新**: 2026-02-12

本文档整合了 NDC 项目的所有文档，提供清晰的导航结构。

## 快速链接

- **[USER_GUIDE.md](./USER_GUIDE.md) - 用户使用指南**
- **[GRPC_CLIENT.md](./GRPC_CLIENT.md) - gRPC 客户端指南
- **[NDC_AGENT_INTEGRATION_PLAN.md](./NDC_AGENT_INTEGRATION_PLAN.md) - AI Agent 集成计划
- **[LLM_INTEGRATION.md](./LLM_INTEGRATION.md) - LLM 提供商集成方案

## 项目概述

NDC (Neo Development Companion) 是一个工业级自治 AI 开发系统，采用 OpenCode 模式的流式响应与多工具集成能力。

### 核心特性

- **工业级 AI Agent**: 无需人工干预即可完成复杂任务
- **流式 LLM 集成**: 支持 OpenAI/Anthropic/Ollama/MiniMax/OpenRouter
- **多工具系统**: MCP/Skills/OpenCode 可扩展工具生态
- **分布式架构**: 支持多 Agent 协作和任务分发
- **知识库持久化**: 基于 Gold Memory 机制的学习和积累
- **任务验证**: 内置 Quality Gates 保证代码质量

### 文档结构

```
docs/
├── README.md           # 本文档（导航页）
├── TODO.md             # 开发计划和进度追踪
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
| runtime/tools | crates/runtime/src/tools/ | MCP, Skills, File, Git, Shell |
| interface | crates/interface/src/ | CLI, REPL, gRPC, Interactive |
| bin | crates/bin/src/ | 二进制入口 |

## 快速开始

```bash
# 构建项目
cargo build --release

# 运行测试
cargo test --release

# 启用 gRPC 功能
cargo build --release --features grpc

# 查看帮助
./target/release/ndc --help
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

> **提示**: 首次使用请阅读 [USER_GUIDE.md](./USER_GUIDE.md) 了解详细用法。

---
