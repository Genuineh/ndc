# NDC 实现待办清单

> **重要更新 (2026-02-12)**: P7.1 Saga 模式工作流状态机 已完成！✅
> **重要更新 (2026-02-12)**: P7.3 Agent 配置持久化系统 已完成！✅
> **重要更新 (2026-02-12)**: P7.4 交互层基础组件 已完成！✅

## 快速开始

```bash
# 1. 构建项目
cargo build --release
```

## 核心设计理念

```
ndc/
├── core/              # 核心模型 + LLM Provider + Memory + Agent + Tools
├── decision/          # 决策引擎
├── runtime/           # 运行时 + 执行器 + 工具系统
├── interface/         # CLI + Daemon
└── bin/              # 二进制文件
```

## 九大阶段

1. 谱系继承 → 继承历史知识 ← ✅ **P1** (Discovery Phase) 已完成
2. 理解需求 → 检索知识库 + 查询 TODO ← ✅ **P6** (Knowledge Understanding) 已完成
3. 分解任务 → LLM 分解 + 模型选择 + 不确定性校验 ← ✅ **P2** (Model Selector) 已完成
4. 影子探测 → 读取代码库 + 影子分析 ← ✅ **P3** (OpenCode Tools) 已完成
5. 工作记忆 → 简洁上下文注入 ← ✅ **P2.2** (Knowledge Injectors - Working Memory + Invariants + Lineage) 已完成
6. 执行开发 → 质量门禁 + 重试机制 ← ✅ **P4** (Quality Gates) 已完成
7. 失败归因 → 人工纠正 → Invariant (Gold Memory) ← ✅ **P3** (Human Correction) 已完成
8. 更新文档 → Fact/Narrative 生成 ← ✅ **P6** (Documentation Updater) 已完成
9. 建立映射 → 关联任务创建与总 TODO 管理 ← ✅ **P7.2** (Knowledge Injectors 集成) 已完成

## 已完成模块 ✅

| 模块 | 文件 | 状态 | 说明 |
|------|------|------|
| **core** | task.rs | ✅ | Task, TaskState, ExecutionStep, ActionResult |
| **core** | intent.rs | ✅ | Intent, Verdict, PrivilegeLevel, Effect |
| **core** | agent.rs | ✅ | AgentRole, AgentId, Permission |
| **core** | memory.rs | ✅ | MemoryStability, MemoryQuery, MemoryEntry |
| **core** | config.rs | ✅ | YAML 配置系统 |
| **core** | ai_agent/mod.rs | ✅ | AI Agent 模块 (Orchestrator, Session, Verifier) |
| **core** | ai_agent/orchestrator.rs | ✅ | Agent Orchestrator - LLM 交互中央控制器 |
| **core** | ai_agent/session.rs | ✅ | Agent Session Manager - 会话状态管理 |
| **core** | ai_agent/verifier.rs | ✅ | Task Verifier - 任务完成验证与反馈循环 |
| **core** | ai_agent/prompts.rs | ✅ | System Prompts - 系统提示词构建 (EnhancedPromptContext) |
| **decision** | engine.rs | ✅ | DecisionEngine, validators |
| **runtime** | executor.rs | ✅ | Task execution, tool coordination |
| **runtime** | workflow.rs | ✅ | State machine, transitions |
| **runtime** | storage.rs | ✅ | In-memory storage |
| **runtime** | storage_sqlite.rs | ✅ | SQLite storage |
| **runtime** | tools/mod.rs | ✅ | Tool, ToolManager |
| **runtime** | tools/fs.rs | ✅ | File operations |
| **runtime** | tools/git.rs | ✅ | Git operations |
| **runtime** | tools/shell.rs | ✅ | Shell command execution |
| **runtime** | tools/ndc/ | ✅ | NDC Task Tools (create/update/list/verify) |
| **runtime** | verify/mod.rs | ✅ | QualityGateRunner |
| **interface** | cli.rs | ✅ | CLI commands |
| **interface** | daemon.rs | ✅ | gRPC service framework |
| **interface** | grpc.rs | ✅ | gRPC service impl |
| **interface** | agent_mode.rs | ✅ | Agent REPL 模式 (P7 集成) |
| **bin/tests** | e2e/mod.rs | ✅ | E2E 测试套件 (217 个测试全部通过) |
| **interface** | repl.rs | ✅ | REPL mode (已集成 Agent 支持) |
| **interface** | e2e_tests.rs | ✅ | E2E tests |

## 待实现功能 (按优先级)

### 🔴 高优先级 - 核心功能缺失

| 模块 | 功能 | 优先级 | 说明 |
|------|------|------|
| runtime/ | Workflow State Machine | 高 | ✅ 实现 Saga 模式工作流状态机 (Saga, SagaStep, SagaOrchestrator, Compensation) |
| runtime/ | Agent Configuration | 高 | ✅ 实现 Agent 配置持久化 (AgentProfile, AgentRoleSelector, AgentConfigDir) |
| interface/ | Interactive Layer | 高 | ✅ 实现基本交互组件 (StreamingDisplay, ProgressIndicator, display_agent_status) |
| interface/ | Service Layer | 高 | ✅ 完善 gRPC 服务框架和客户端 SDK (proto 定义, 流式 RPC, StreamingChat, StreamExecuteTask) |
| runtime/ | LLM Integration | 高 | ✅ 扩展 LLM Provider 支持，实现流式响应 (complete_streaming, StreamHandler, process_streaming) |

### 🟠 中优先级 - 增强功能

| 模块 | 功能 | 优先级 | 说明 |
|------|------|------|
| runtime/ | Knowledge Persistence | 中 | 实现知识库持久化存储，支持知识更新和查询 |
| runtime/ | Multi-Model Support | 中 | 实现多模型并行推理，降低 LLM 不确定性 |
| runtime/ | Memory Compression | 中 | 实现上下文压缩优化，减少 Token 消耗 |
| runtime/ | Tool Caching | 中 | 实现工具结果缓存，提升重复操作效率 |
| ai_agent/ | Task Validation | 中 | 增强任务验证逻辑，支持更复杂的验证规则 |

### 🟡 低优先级 - 体验优化

| 模块 | 功能 | 优先级 | 说明 |
|------|------|------|
| runtime/ | Progress Indicators | 低 | 实现任务进度可视化、ETA 显示 |
| runtime/ | Error Recovery | 低 | 完善错误恢复机制，支持自动重试和降级 |
| runtime/ | Logging Enhancement | 低 | 增强结构化日志，支持日志级别和格式化输出 |
| interface/ | CLI UX | 低 | 改进命令行体验，增加自动补全和帮助提示 |
| interface/ | REPL History | 低 | 实现命令历史记录、搜索和重放功能 |

### 📝 待规划 - 长期架构演进

| 阶段 | 说明 | 状态 |
|------|------|------|
| **Phase 10** | 自主 Agent | 规划 | 实现 Agent 自主规划能力，无需人类干预即可完成复杂任务 |
| **Phase 11** | 分布式 Agent | 规划 | 实现多 Agent 协作，支持分布式任务拆分和执行 |
| **Phase 12** | 联邦学习 | 规划 | 从历史执行中学习，优化决策模式 |
| **Phase 13** | 工具生态 | 规划 | 扩展标准工具协议，支持第三方工具集成 |
| **Phase 14** | 边界安全 | 规划 | 实现 Agent 沙箱隔离和权限管理 |
| **Phase 15** | 成本优化 | 规划 | 优化资源使用，实现按需计费模式 |

## 快速参考

### 常用命令

```bash
# 所有测试
cargo test --release
```

### 开发指南

#### 代码规范

1. **错误处理**: 使用 `Result<T>` 和 `?` 操作符，避免 unwrap
2. **异步设计**: 使用 `async fn` 和 `.await`，避免阻塞
3. **日志记录**: 使用 `tracing::info/warn/error` 替代 `println!`
4. **配置管理**: 所有配置项通过结构体定义，使用 `derive(Debug, Clone)`
5. **测试编写**: 每个模块应包含单元测试，覆盖主要逻辑路径

#### Git 工作流

```bash
# 功能开发
git checkout -b feature/<branch-name>
git commit -m "type(scope): message"

# 文档更新
echo "### 更新时间: $(date +%Y-%m-%d)" >> docs/TODO.md
```

## 项目统计

- **总代码行数**: 约 15,000+ 行 Rust 代码
- **测试覆盖**: 217 个测试全部通过
- **文档完整度**: 完整的架构设计文档和开发指南
- **开发语言**: Rust 2021 edition
- **项目周期**: 自主开发，无外部依赖

---

> **注意**: 本文档由 AI Agent 自动维护，反映当前实际开发状态和计划。
