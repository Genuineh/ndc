# NDC TODO / Backlog

> 更新时间：2026-02-25（v6）  
> 关联文档：`docs/plan/current_plan.md`、`docs/USER_GUIDE.md`、`docs/design/p0-d-security-project-session.md`

## 看板总览

- `P0-D`（最高优先级，进行中）：安全边界与项目级会话隔离（对齐 OpenCode）
- `P0-C`（已完成）：Workflow-Native REPL 与实时可观测
- `P1`（次高优先级，进行中）：核心自治能力与治理
- `P2`（后续增强，待开始）：多 Agent 与知识回灌体验

## P0-D（最高优先级：安全边界与项目级会话隔离）

设计导航：
- 详细设计：`docs/design/p0-d-security-project-session.md`
- 严格验收门禁：`docs/design/p0-d-security-project-session.md#5-strict-acceptance-gates-blocking`
- 测试矩阵：`docs/design/p0-d-security-project-session.md#6-test-plan-and-matrix`

目标：
- 危险操作可控：对 shell/fs/git 的高风险行为进行统一判定、确认与拒绝。
- 项目上下文隔离：不同项目（A/B）必须拥有独立 session 上下文、历史和 resume 入口。
- resume 正确归属：在项目 A 中 continue/resume 只能回到 A 的会话，不串到其他项目。
- 与 OpenCode 对齐：参考 `opencode/specs/project.md`、`opencode/packages/opencode/src/project/*`、`opencode/packages/opencode/src/permission/*` 的设计语义。

### 现状审计（2026-02-25）

- 已有边界（可复用）：
  - `interface::ReplToolExecutor` 已支持按工具分类的 `allow/ask/deny`。
  - `runtime::tools::permission` 与 `bash_parsing` 已有危险级别与危险模式基础能力。
- 关键缺口（必须补齐）：
  - `runtime::tools::permission` 尚未成为统一强制入口（当前主要依赖 REPL 层分类确认）。
  - 缺少 `external_directory`（项目外目录访问）边界判定与专门权限语义。
  - 已有 `project_id` 与项目级索引首批实现，但尚未完成跨进程持久化索引。
  - `run --continue/--session` 已接入同项目恢复与跨项目默认拒绝，仍需补齐 daemon/E2E 端到端回归。

### P0-D 当前执行清单

1. `P0-D1` 项目身份识别与上下文归属
   - 设计章节：`3.1`、`4 (P0-D1)`
   - 引入稳定 `project_id`（优先 git 根标识，回退目录指纹）。
   - 在 session 元数据中持久化 `project_id/worktree/directory`。
   - REPL 状态栏与 `/status` 显示当前项目标识。
2. `P0-D2` 项目级 session 索引与 resume
   - 设计章节：`3.2`、`3.3`、`4 (P0-D2)`
   - 建立 `project_id -> sessions` 索引与“最近会话”游标。
   - `/resume`、`--continue` 默认仅恢复当前项目的最近 root session。
   - `--session <id>` 增加跨项目保护（默认拒绝，提供显式 override 开关）。
3. `P0-D3` 危险操作统一权限网关
   - 设计章节：`3.4`、`3.5`、`4 (P0-D3)`
   - 将 `PermissionSystem + BashParser` 接入工具执行主入口（非仅 REPL UI）。
   - 新增 `external_directory` 权限类型与判定（路径超出项目根时触发）。
   - 对 `Critical` 默认拒绝；`High` 强制确认；`Medium` 可配置确认策略。
4. `P0-D4` REPL 实时安全可观测
   - 设计章节：`3.6`、`4 (P0-D4)`
   - 在 session 面板实时展示：`permission_asked/approved/rejected`、风险级别、目标路径/命令。
   - 权限等待状态显式阻塞标记，避免“看起来卡住”。
5. `P0-D5` 测试与回归
   - 设计章节：`5`、`6`、`4 (P0-D5)`
   - 单测：项目 ID 计算、路径越界判定、危险命令分级、跨项目 resume 防串线。
   - 集成/E2E：项目 A/B 并行会话、A 内 resume 回 A、external directory 触发确认。
6. `P0-D6` 文档与配置
   - 设计章节：`3.7`、`4 (P0-D6)`、`8`
   - 更新 `docs/USER_GUIDE.md`：安全模型、权限策略、项目级 resume 规则。
   - 更新 `docs/plan/current_plan.md`：P0-D 里程碑、验收门禁与迁移说明。

### P0-D 最新进展（2026-02-25）

- 已完成（P0-D1 首批）：
  - `core` 新增 `ProjectIdentity` 解析（git root commit / non-git path fingerprint）。
  - `AgentSession` 新增项目元数据字段：`project_id/project_root/working_dir/worktree`。
  - `orchestrator` 创建新会话时已注入项目身份。
  - REPL 状态栏与 `/status` 已展示 `project` 标识。
- 已完成（P0-D2 首批）：
  - `orchestrator` 新增项目会话索引与最近会话游标（`project_sessions/project_last_root_session`）。
  - `orchestrator` 新增 `session_project_identity` 查询接口，供 interface 做会话归属校验。
  - `AgentModeManager` 新增会话控制 API：`start_new_session`、`resume_latest_project_session`、`use_session`。
  - REPL 新增 `/new`、`/resume`（支持 `/resume <id> [--cross]`），并接入 hints/补全。
  - CLI `run` 已接入 `--continue`、`--session <id>`、`--allow-cross-project-session`。
  - 新增回归测试：跨项目会话拒绝、项目最近会话游标、REPL `/resume` 参数补全、manager 会话控制。
- 已完成（P0-D2 补齐：daemon/gRPC 一致性）：
  - gRPC `GetSessionTimeline/SubscribeSessionTimeline` 已接入 `AgentModeManager::use_session`，不再仅允许“当前活跃会话”。
  - SSE `/agent/session_timeline/subscribe` 已接入同样的 session 归属校验（同项目可切换，跨项目默认拒绝）。
  - 新增回归测试：同项目“非活跃旧 session”可被 gRPC/SSE 访问；无效 session 仍返回 `404`。
- 已完成（P0-D3 首批：统一权限网关）：
  - runtime 新增 `tools::security` 网关，统一处理：
    - `external_directory` 边界判定（默认 `ask` 语义）
    - shell 风险分级（`Critical` deny、`High` ask、`Medium` 可配置）
    - git `commit` 默认 `ask`
  - 已接入工具主链：`shell/fs/git/read/write/edit/list/glob/grep`（非仅 REPL UI）。
  - 新增单测：外部目录拒绝、shell critical 拒绝、git commit ask。
- 已完成（P0-D3 第二批：REPL 确认闭环首版）：
  - runtime `ask` 错误改为可解析格式：`requires_confirmation permission=<...> risk=<...> ...`。
  - 新增单次授权覆盖机制（task-local）：`with_security_overrides(...)`。
  - `ReplToolExecutor` 收到 runtime `requires_confirmation` 后可确认并重试（非仅返回拒绝提示）。
  - 新增回归测试：`test_runtime_permission_ask_can_auto_confirm_and_retry`。
- 已完成（P0-D4 首批：状态栏权限可观测增强）：
  - REPL 状态栏新增 `perm_state/perm_type/perm_risk` 字段。
  - Permission 事件行新增结构化标签：`[state=...][type=...][risk=...]`。
  - orchestrator 已补齐权限事件链：`permission_asked -> permission_approved/rejected -> tool_call_end`。
  - 新增回归测试：permission 事件解析与状态切换。
- 已完成（P0-D5 首批：gRPC/SSE 权限事件一致性回归）：
  - 新增 `grpc` 映射/序列化回归：`permission_asked/approved/rejected` 在 `map_execution_event` 与 `execution_event_to_json` 路径保持一致。
  - `SubscribeSessionTimeline` vs `GetSessionTimeline` replay 断言新增 `message` 字段一致性校验，避免权限事件语义在订阅回放中丢失。
- 已完成（P0-D5 第二批：daemon 跨进程权限事件回归）：
  - 新增回归测试：跨 manager/重启场景下，gRPC `GetSessionTimeline` 可读取持久化会话中的权限事件链（`permission_asked/approved/rejected`）。
  - 新增回归测试：SSE `/agent/session_timeline/subscribe` 可回放同一持久化权限事件链，并保持字段语义一致。
  - 修复 SSE 测试连接抖动：测试 HTTP 客户端新增短时重试，避免服务启动竞态导致 `ConnectionRefused` 偶发失败。
- 已完成（P0-D6 首批：非交互通道确认策略落地）：
  - `ReplToolExecutor` 在无 TTY 场景不再尝试 stdin 阻塞确认，返回结构化拒绝：`non_interactive confirmation required: ...`。
  - 新增回归测试：`test_runtime_permission_retry_non_interactive_returns_denied`。
- 已完成（P0-D1/P0-D2 第二批：REPL 项目识别与切换引导）：
  - REPL 启动时展示当前识别项目：`project_id/project_root/session`，并提示项目导航入口。
  - 新增 `/project` 命令族：
    - `/project status`（当前项目上下文）
    - `/project list`（发现附近项目并编号）
    - `/project use <index|path>`（切换项目并绑定/恢复 session）
    - `/project sessions [index|project-id]`（查看项目会话）
  - 命令补全与 hints 已接入 `/project` 参数提示与用法引导。
- 已完成（P0-D1/P0-D2 第三批：项目导引强化与快捷切换）：
  - REPL 启动首屏补充“附近项目列表（带 index）”与“当前项目最近 session 列表”，并标记当前 active 项。
  - TUI 项目选择器新增 active 项标识与选中项目详情，降低误切换风险。
  - 新增 `Ctrl+P` 快捷键（可通过 `NDC_REPL_KEY_OPEN_PROJECT_PICKER` 覆盖）直接打开项目选择器。
  - 启动引导文案补强：明确 `/project pick`、`/project use`、`/project sessions`、`/resume` 的串联路径。
- 已完成（P0-D1/P0-D2 第四批：跨进程项目索引持久化与快速恢复）：
  - 新增持久化项目索引：`~/.config/ndc/project_index.json`（可用 `NDC_PROJECT_INDEX_FILE` 覆盖）。
  - `enable/process_input/switch_project_context/start_new_session/resume/use_session` 全链路写入项目索引，记录最近活跃项目与会话指针。
  - `discover_projects` 已接入持久化索引种子，重启进程后仍可恢复“已知项目”候选（并保持当前项目优先）。
  - `known_project_ids` 已合并内存索引 + 持久化索引，跨进程项目识别更稳定。
  - 新增回归测试：项目索引 roundtrip、持久化索引驱动 discover/known_project_ids。
- 已完成（P0-D1/P0-D2 第五批：跨进程会话归档与启动恢复）：
  - 新增持久化会话归档：`~/.config/ndc/session_archive.json`（可用 `NDC_SESSION_ARCHIVE_FILE` 覆盖）。
  - `enable` 会加载归档并 hydrate 到 orchestrator，默认恢复“当前项目最近会话”。
  - `process_input` 成功后会将当前会话快照回写归档，包含消息与 execution timeline。
  - 新增回归测试：会话归档 roundtrip、跨 manager 启动后自动恢复 session 与 timeline。
- 已完成（P0-D3 补强：项目切换后的工具上下文生效）：
  - `ReplToolExecutor` 对 `shell/fs` 注入当前项目 `working_dir`，避免“只切 UI 不切执行上下文”。
  - `ShellTool` 新增可选 `working_dir` 参数，执行目录与安全判定目录统一。
  - `PromptBuilder` 新增 `Project Context` 段落，向模型显式注入当前工作目录。
- 下一步：
  - 推进 `P0-D6`：补充“非交互通道确认策略”迁移说明与运维默认值建议。

## P0-C（已完成：Workflow-Native REPL 与实时可观测）

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
4. 执行链与安全边界修复
   - 修复 `Executor` 执行 intent 时误用 `task.clone()` 导致步骤结果丢失的问题（现已回写到真实 task）。
   - 安全边界判定已支持以 `working_dir/project_root` 作为项目根提示，避免测试/多项目上下文下误判 `external_directory`。
   - `USER_GUIDE` 已修正会话订阅语义：同项目非活跃 session 可自动切换并返回 `200`。

## P1（次高优先级）

1. GoldMemory 检索结果接入 orchestrator 自动上下文选择（按任务上下文注入 Top-K facts）
2. Failure Taxonomy 接入重试与回滚策略（含 NonDeterministic）
3. Invariant 的 TTL/version/conflict 检查接入执行前阶段
4. Telemetry 指标落地（autonomous_rate / intervention_cost / token_efficiency）
5. MCP/Skills 接入默认工具发现链与权限治理链

### P1 当前执行清单（P0-D 完成后推进）

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
