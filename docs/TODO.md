# NDC TODO / Backlog

> 更新时间：2026-02-25（v2）  
> 关联文档：`docs/plan/current_plan.md`、`docs/USER_GUIDE.md`

## 看板总览

- `P0-C`（最高优先级，已完成）：Workflow-Native REPL 与实时可观测
- `P1`（当前最高优先级，进行中）：核心自治能力与治理
- `P2`（后续增强，待开始）：多 Agent 与知识回灌体验

## P0-C（最高优先级：Workflow-Native REPL 与实时可观测）

目标：
- 终端实时看到：`当前 workflow 阶段 + 阶段进度 + 本轮/累计 token + 工具耗时/错误分布`
- REPL / gRPC / SSE 使用同一套观测语义

### 已完成（持续推进）

- `core` 已固化 `AgentWorkflowStage` 作为阶段真相源（planning/discovery/executing/verifying/completing）
- orchestrator 主循环已发射阶段事件并覆盖多轮
- `AgentExecutionEvent` 已补齐结构化 workflow 字段：`workflow_stage/workflow_detail/workflow_stage_index/workflow_stage_total`
- REPL / gRPC 已优先消费结构化 workflow 字段（message 仅作人类可读回退）
- token usage 已接入（provider 优先，缺失回退 estimate）
- REPL 状态栏已展示：
  - `workflow`
  - `workflow_progress`
  - `workflow_ms`
  - `blocked`
  - `tok_round/tok_session`
- REPL `/workflow`、`/tokens`、`/metrics` 已可用
- REPL `/workflow` 已支持 `compact|verbose` 双视图（默认 `verbose`）
- 命令提示补全已覆盖 `/workflow` 参数（`compact/verbose`）
- workflow 阶段耗时统计已补边界：
  - 当前阶段无后继事件时，`total_ms` 会累计 `active_ms`
  - 历史缓存达到上限时会提示“统计可能不完整”
- Session 面板与 `/timeline` 已按 `[stage:<name>]` 分段展示
- gRPC/SSE `ExecutionEvent` 已扩展字段：
  - workflow：`workflow_stage/workflow_detail/workflow_stage_index/workflow_stage_total`
  - token：`token_source/token_prompt/token_completion/token_total/token_session_prompt_total/token_session_completion_total/token_session_total`
- 兼容策略文档已补齐（旧客户端忽略新增字段可降级）
- 已新增综合 e2e（core orchestrator）：多轮 + 多次 tool call + permission + timeline replay + workflow/token 断言
- 已新增 interface 侧结构化字段测试（REPL 渲染 + gRPC 映射）
- 已新增订阅重放一致性 e2e（`SubscribeSessionTimeline` vs `GetSessionTimeline` 字段一致）
- 单测已覆盖 core/interface/grpc 关键路径

### 待完成（当前执行清单）

- 无（P0-C 已完成）

## 最近修复（2026-02-25）

1. Provider 凭证读取修复
   - MiniMax 别名 provider（`minimax-coding-plan`、`minimax-cn-*`）已支持配置键归一化回退（`minimax`），避免配置凭证被跳过导致鉴权失败。
2. 任务状态机约束修复
   - `ndc_task_update` 不再允许非法强制迁移；非法迁移会明确报错并保持原状态不变。
3. 回归测试修复
   - tool 链 smoke 改为合法状态链：`preparing -> in_progress -> awaiting_verification -> completed`。

## P1（高优先级）

1. GoldMemory 检索结果接入 orchestrator 自动上下文选择（按任务上下文注入 Top-K facts）
2. Failure Taxonomy 接入重试与回滚策略（含 NonDeterministic）
3. Invariant 的 TTL/version/conflict 检查接入执行前阶段
4. Telemetry 指标落地（autonomous_rate / intervention_cost / token_efficiency）
5. MCP/Skills 接入默认工具发现链与权限治理链

### P1 当前执行清单（下一阶段）

1. `P1-1` GoldMemory Top-K 注入主链
   - 在 orchestrator prompt 构建前注入 task 相关 Top-K facts
   - 增加命中率与上下文长度边界测试
2. `P1-2` 失败分类驱动重试
   - 将 `Logic/TestGap/SpecConflict/NonDeterministic` 接入重试决策
   - 为 NonDeterministic 配置回退与人工介入阈值
3. `P1-3` 执行前 invariant 检查
   - 接入 TTL/version/conflict 检查
   - 非法冲突在执行前阻断并返回结构化原因
4. `P1-4` Telemetry 首批指标
   - 落地 `autonomous_rate/intervention_cost/token_efficiency`
   - REPL/gRPC 输出统一指标快照（只增不破坏兼容）

## P2（后续增强）

1. 多 Agent 协同编排（planner / implementer / reviewer）
2. 文档自动回灌与知识库固化策略（阶段 8）
3. REPL 可视化进度与历史重放

## 已完成里程碑（压缩）

- P0-A：REPL UI 对齐 OpenCode（固定输入区、可滚动 session、快捷键、命令提示补全）
- P0-B：多轮对话实时可视化（事件模型、timeline 回放、实时流、SSE/gRPC、脱敏）
- 主链能力：tool-calling、task verify、memory/invariant 回灌、storage 打通等主线修复

> 详细历史请查看 `git log` 与 `docs/plan/current_plan.md`，本文件仅保留“可执行待办 + 里程碑摘要”。

## 验收门禁（P0/P1 合并前）

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新（本文件 + 相关计划/用户文档）
