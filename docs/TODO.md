# NDC TODO / Backlog

> 更新时间：2026-02-26（v9）  
> 已完成里程碑归档：`docs/plan/archive/COMPLETED_MILESTONES.md`  
> 关联文档：`docs/plan/current_plan.md` · `docs/USER_GUIDE.md` · `docs/design/`

## 看板总览

| 优先级 | 状态 | 主题 |
|--------|------|------|
| **P0-D** | ✅ 已完成 | 安全边界与项目级会话隔离 |
| **P0-C** | ✅ 已完成 | Workflow-Native REPL 与实时可观测 |
| **P1-UX** | ✅ 已完成 | REPL TUI 布局与体验重设计（P1-UX-1~6 全部完成） |
| **P1** | 待开始 | 核心自治能力与治理 |
| **P2** | 待开始 | 多 Agent 与知识回灌体验 |

---

## 活跃工作

### P0-D 收口（安全边界）

> 设计：`docs/design/p0-d-security-project-session.md`  
> P0-D1~D6 全部实现完毕，仅剩验收收口。

- [ ] 按 Gate A/B/C/D 进行一次完整验收回归并归档证据

### P1-UX-2 消息轮次模型（✅ 已完成）

> P1-UX-1~6 已全部完成。

- [x] 引入 `ChatEntry` / `ToolCallCard` 数据模型，替代 `Vec<String>` 日志行
- [x] 用户消息 / 助手回复带视觉边框与轮次标识
- [x] 工具调用渲染为可折叠卡片 `▸/▾ name status duration`
- [x] 推理内容默认折叠

### P1-UX 延期项

- [ ] 权限区独立交互（y/n/a 快捷键）— 需 async channel 重构（当前权限确认走 stdin 阻塞）

---

## P1 待办清单

| # | 任务 | 描述 |
|---|------|------|
| P1-1 | GoldMemory Top-K 注入 | orchestrator prompt 构建前注入 task 相关 Top-K facts |
| P1-2 | 失败分类驱动重试 | `Logic/TestGap/SpecConflict/NonDeterministic` 接入重试决策 |
| P1-3 | 执行前 invariant 检查 | TTL/version/conflict 检查，非法冲突在执行前阻断 |
| P1-4 | Telemetry 首批指标 | `autonomous_rate / intervention_cost / token_efficiency` |
| P1-5 | MCP/Skills 工具发现 | 接入默认工具发现链与权限治理链 |

---

## P2 Backlog

| # | 任务 |
|---|------|
| P2-1 | 多 Agent 协同编排（planner / implementer / reviewer） |
| P2-2 | 文档自动回灌与知识库固化策略 |

---

## 已完成摘要

| 里程碑 | 完成时间 | 概要 |
|--------|----------|------|
| P0-A | 2026-02 | REPL UI 对齐 OpenCode（固定输入区、滚动 session、快捷键、命令补全） |
| P0-B | 2026-02 | 多轮对话实时可视化（事件模型、timeline、SSE/gRPC、脱敏） |
| P0-C | 2026-02 | Workflow-Native REPL（阶段观测、token 统计、gRPC/SSE 一致） |
| P0-D | 2026-02 | 安全边界（项目隔离、权限网关、持久化索引/归档、非交互通道） |
| P1-UX-1 | 2026-02 | TUI 5~6 区动态布局 |
| P1-UX-2 | 2026-02 | 消息轮次模型（ChatEntry/ToolCallCard 替代 Vec<String>、可折叠卡片） |
| P1-UX-3 | 2026-02 | TuiTheme 20 色语义化主题 |
| P1-UX-4 | 2026-02 | 输入历史 / 多行输入 / 焦点分离 / Markdown 渲染 |
| P1-UX-5 | 2026-02 | Token 进度条 / 输出截断 / 启动精简 |
| P1-UX-6 | 2026-02 | 三级 Verbosity / 阶段去重 / 工具概要 / 权限指引 / 轮次分组 |
| 工程治理 | 2026-02 | 清理空 crate、storage 独立、edition 2024 统一 |

> 详细实现记录见 `docs/plan/archive/COMPLETED_MILESTONES.md`

---

## 验收门禁（合并前）

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新
