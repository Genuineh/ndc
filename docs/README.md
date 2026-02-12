# NDC 文档导航

> **最后更新**: 2026-02-12

本文档整合了 NDC 项目的所有文档，提供清晰的导航结构。

## 快速链接

- **[NDC_AGENT_INTEGRATION_PLAN.md](./NDC_AGENT_INTEGRATION_PLAN.md) - AI Agent 集成计划
- **[LLM_INTEGRATION.md](./LLM_INTEGRATION.md) - LLM 提供商集成方案
- **[USER_GUIDE.md](./USER_GUIDE.md) - 用户使用指南
- **[GRPC_CLIENT.md](./GRPC_CLIENT.md) - gRPC 客户端指南
- **[ENGINEERING_CONSTRAINTS.md](./ENGINEERING_CONSTRAINTS.md) - 约束和最佳实践
- **[E2E_TEST_PLAN_V2.md](./E2E_TEST_PLAN_V2.md) - E2E 测试计划

## 项目概述

NDC (Neo Development Companion) 是一个工业级自治 AI 开发系统，采用 OpenCode 模式的流式响应与多工具集成能力。

### 核心特性

- **工业级 AI Agent**: 无需人工干预即可完成复杂任务
- **流式 LLM 集成**: 支持主流 LLM 提供商
- **多工具系统**: MCP/ Skills/OpenCode 可扩展工具生态
- **分布式架构**: 支持多 Agent 协作和任务分发
- **知识库持久化**: 基于 Gold Memory 机制的学习和积累
- **任务验证**: 内置 Quality Gates 保证代码质量

### 文档结构

```
docs/
├── README.md           # 本文档（导航页）
├── TODO.md             # 开发计划和进度追踪
├── INTEGRATION_PLAN.md  # AI Agent 集成方案
└── ...
```

## 模块说明

| 模块 | 文档 | 说明 |
|------|------|------|
| ai_agent/ | AI Agent 核心模块 | Agent Orchestrator, Session, Verifier, Prompts, Adapters |
| core/task/ | 任务管理核心 | Task, TaskState, ExecutionStep |
| core/llm/ | LLM Provider 系统 | Provider, ModelSelector |
| runtime/tools/ | 工具系统 | MCP, Skills, File, Git, Shell |
| interface/ | 接口层 | CLI, Daemon, gRPC |
| bin/ | 二进制入口 | CLI, REPL, Agent Mode |

### 快速开始

```bash
# 构建项目
cargo build --release
```

### 开发指南

#### 代码规范

1. **错误处理**: 使用 `Result<T>` 和 `?` 操作符
2. **异步设计**: 使用 `async fn` 和 `.await`
3. **日志记录**: 使用 `tracing::info/warn/error`
4. **配置管理**: 使用结构体和 `derive`
5. **测试编写**: 每个模块包含单元测试

#### Git 工作流

```bash
# 功能开发
git checkout -b feature/<branch-name>
git commit -m "type(scope): message"
```

---
