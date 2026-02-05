# NDC 实现待办清单

> 基于 `docs/design/2026-02-04-ndc-final-design.md` 提取

## Phase 1: 最小可运行核心 (MRC) [P0]

### 1.1 配置 DevMan 依赖
- [ ] 添加 git 依赖到 workspace Cargo.toml
- [ ] 验证编译通过
- [ ] 文档: `docs/development/setup.md`

### 1.2 核心数据模型 (`crates/core`)
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | Intent, Verdict, Effect | ☐ | 决策引擎核心类型 |
| P0 | Agent, AgentRole | ☐ | 角色定义 |
| P0 | MemoryEntry, MemoryStability | ☐ | 记忆模型 |
| P0 | Task, TaskState | ☐ | 任务模型 |

### 1.3 决策引擎 (`crates/decision`)
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | DecisionEngine trait | ☐ | 核心接口 |
| P0 | TaskBoundaryValidator | ☐ | 任务边界校验 |
| P0 | PermissionValidator | ☐ | 权限校验 |
| P1 | SecurityPolicyValidator | ☐ | 安全策略 |
| P1 | DependencyValidator | ☐ | 依赖校验 |

### 1.4 Adapter 层 (`crates/adapter`)
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | TaskAdapter | ☐ | Intent → DevMan Task |
| P0 | QualityAdapter | ☐ | 质量检查适配 |
| P1 | KnowledgeAdapter | ☐ | 知识服务适配 |
| P1 | ToolAdapter | ☐ | 工具执行适配 |

### 1.5 CLI 基础 (`crates/cli`)
| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | `ndc create <task>` | ☐ | 创建任务 |
| P0 | `ndc status` | ☐ | 查看状态 |
| P0 | `ndc list` | ☐ | 列出任务 |

---

## Phase 2: 完整适配 [P1]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P1 | KnowledgeAdapter 完整实现 | ☐ | Memory → Knowledge |
| P1 | ToolAdapter 完整实现 | ☐ | Action → Tool |
| P1 | AsyncTaskAdapter | ☐ | JobManager 集成 |

---

## Phase 3: 交互层 [P2]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P2 | REPL 模式 (`crates/repl`) | ☐ | 对话式交互 |
| P2 | 守护进程 (`crates/daemon`) | ☐ | gRPC 服务器 |
| P2 | CLI 控制命令 | ☐ | 完整命令集 |

---

## Phase 4: 可观测性 [P2]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P2 | Task Timeline | ☐ | 任务时间线 |
| P2 | Agent 行为日志 | ☐ | 操作轨迹 |
| P2 | 记忆访问轨迹 | ☐ | 上下文访问 |

---

## Phase 5: 生产就绪 [P3]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P3 | 完整测试覆盖 | ☐ | > 80% |
| P3 | 性能优化 | ☐ | 基准测试 |
| P3 | 安全审计 | ☐ | 代码审查 |
| P3 | 用户文档 | ☐ | README, guides |

---

## 插件系统 [P3]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P3 | MCP 协议支持 | ☐ | Model Context Protocol |
| P3 | Skills 系统 | ☐ | 工作流模板 |
| P3 | WASM 沙箱 | ☐ | 自定义扩展 |

---

## 外部依赖 (DevMan Issues)

| Issue | 功能 | 状态 | 优先级 |
|-------|------|------|--------|
| #1 | 知识稳定性 | 已提交 | 中 |
| #2 | 访问控制 | 已提交 | 中 |
| #3 | 向量检索 | 已提交 | 高 |

---

## 优先级说明

| 标记 | 含义 |
|------|------|
| P0 | 必须完成，MVP 核心 |
| P1 | 重要功能，后续阶段依赖 |
| P2 | 增强体验，可后续 |
| P3 | 优化项，可选实现 |

---

最后更新: 2026-02-04
标签: #ndc #todo #planning
