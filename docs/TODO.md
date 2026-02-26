# NDC TODO / Backlog

> 更新时间：2026-02-26（v8）  
> 关联文档：`docs/plan/current_plan.md`、`docs/USER_GUIDE.md`、`docs/design/p0-d-security-project-session.md`、`docs/design/p0-d6-non-interactive-migration.md`

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
- 已完成（P0-D6 第二批：迁移说明与运维默认值）：
  - 新增迁移文档：`docs/design/p0-d6-non-interactive-migration.md`（含 channel 语义矩阵、推荐配置档位、上线检查清单）。
  - `USER_GUIDE` 新增 `6.2`，明确非交互通道行为、推荐 env、以及测试模式默认值注意事项。
  - runtime 新增回归测试：网关默认启用、测试模式默认值、CSV 覆盖解析。
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
  - `P0-D` 收口：按 `Gate A/B/C/D` 进行一次完整验收回归并归档证据。

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

1. 工程治理重构
   - 移除 8 个空占位 crate 目录（cli, context, daemon, execution, observability, plugins, repl, task）
   - 从 runtime 抽取独立 `ndc-storage` crate（Storage trait + MemoryStorage + SqliteStorage）
   - runtime 通过依赖 `ndc-storage` 复用存储层，不再内联 storage 模块
   - 全 workspace 统一 Rust edition 2024（`edition.workspace = true`）
   - 修复 edition 2024 match ergonomics 与 unsafe env var 变更
2. Provider 凭证读取修复
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
6. **REPL TUI 布局与体验重设计**（P1-UX）— P1-UX-1/3/4/5/6 已完成，P1-UX-2（轮次模型）待定

### P1-UX（REPL TUI 布局与体验重设计）

设计导航：`docs/design/p1-repl-ux-redesign.md`

目标：
- 从"日志行"体验升级为"对话轮次"体验，对齐 OpenCode 等现代 AI 编码助手的交互风格。
- 解决当前布局信息过密、视觉扁平、缺少结构层次感、Hints 区浪费空间等问题。

### P1-UX 最新进展（2026-02-26）

- 已完成（P1-UX-1 布局重构）：
  - 新 5~6 区动态布局已落地：标题栏(1) → 工作流进度条(1) → 对话区(Min5) → 权限栏(条件2) → 状态提示栏(1) → 输入区(动态3~6)。
  - `tui_layout_constraints(has_permission, input_lines)` 动态返回约束向量，权限栏按需 0/2 行，输入区高度随多行输入自动扩展。
  - `build_title_bar()` 精简为核心 4 项：品牌标识、项目名、会话 ID、模型、状态。
  - `build_workflow_progress_bar()` 显示五阶段 pipeline（planning ── discovery ── [executing] ── verifying ── completing）。
  - `build_permission_bar()` 权限等待时显示 ⚠ 图标 + 操作提示（y/n/a）+ 待授权消息。
  - `build_status_hint_bar()` 合并旧 Hints(4行)+Status(1行) 为 1 行上下文敏感提示：斜杠补全态/token进度条态/默认快捷键态。
  - 输入区去掉冗长标题，改为简洁 `>` 前缀 + 主题边框。
  - 旧 `build_status_line`/`build_input_hint_lines` 标记 `#[cfg(test)]` 保留测试兼容。
- 已完成（P1-UX-3 主题系统）：
  - `TuiTheme` 20 个语义化颜色变量。
  - `TuiTheme::default_dark()` 提供深色终端默认配色方案。
  - `style_session_log_line()` 全部颜色已改为 theme 引用，不再硬编码 `Color::*`。
  - 消息行新增视觉图标：`▌`（角色标记）、`◆ ✗ ✓ ▸ ◇ ◌ →`（状态/工具/流程指示符）。
  - `tool_status_narrative()` 按工具类型输出语义化文案。
- 已完成（P1-UX-4 交互增强）：
  - `InputHistory` 循环缓冲（去重、容量上限 100、草稿保存）。
  - `↑`/`↓` 在输入区导航历史；`Ctrl+↑`/`Ctrl+↓` 滚动对话区（焦点分离）。
  - 多行输入：`Shift+Enter` / `Alt+Enter` 插入换行，输入区动态扩展至 4 行上限。
  - 权限消息提取：`PermissionAsked` 事件设置 `permission_pending_message`，非授权事件自动清除。
  - 基础 Markdown 渲染：`render_inline_markdown()` 支持 # 标题、- 列表（→•）、代码围栏（```）、行内 `code`、**粗体**、*斜体*。
- 已完成（P1-UX-5 polish）：
  - Token 使用进度条：`format_token_count()` 人性化显示 (1.5k/32.0k)，`token_progress_bar()` 8字符可视化 [████░░░░]。
  - 长输出截断：工具输出超过 200 字符时显示 `… (truncated)` 后缀。
  - 启动信息精简为单行："NDC — describe what you want, press Enter. /help for commands"。
- 已完成（P1-UX-6 过程展示优化）：
  - `DisplayVerbosity { Compact, Normal, Verbose }` 三级详细度模型，`Ctrl+D` 循环切换，`/verbosity` 命令直接设置。
  - `event_to_lines()` 完全重写：按 verbosity 级别差异化输出所有事件类型。
  - 阶段切换去重：Compact 单行 `◆ Planning...`，Normal 附带 detail，Verbose 保留原始双行。
  - 工具调用单行概要：`extract_tool_summary()` 从 JSON 参数提取人性化摘要（shell→command, read→path 等）。
  - Token 使用分级：Compact 隐藏（状态栏已有），Normal `tok +N (M total)`，Verbose 原始 k=v。
  - 权限交互增强：`[PermBlock]` 高亮 + `[PermHint]` 操作指引。
  - 轮次分组：Normal/Verbose 模式在 round 切换时插入 `── Round N ──` 分隔线。
  - `style_session_log_line()` 新增 6 种行前缀样式：`[RoundSep]`(dim)、`[Stage]`(primary+bold ◆)、`[ToolRun]`(tool_accent ▸)、`[ToolEnd]`(✓/✗)、`[PermBlock]`(⚠)、`[PermHint]`(muted)。
  - `format_duration_ms()` 人性化时长格式（450ms / 1.5s / 1.0m）。
- 已完成（测试）：
  - 116 个 repl 测试通过（含 21 个 P1-UX-6 新增 + 15 个已有测试适配）。
  - P1-UX-6 新增测试覆盖：`DisplayVerbosity` parse/cycle/label、`capitalize_stage`、`format_duration_ms`、`extract_tool_summary`（shell/read/grep/unknown）、verbosity 分级行为（compact 隐藏 steps/tokens、normal 显示 tokens、stage 单行/双行、round separator、permission hints、compact tool summary）。
  - 全量 workspace 测试通过（2 个 agent_mode 预存失败不受影响）。

执行分 6 个 Phase：

1. ~~`P1-UX-1` 结构改造（基础布局）~~ ✅ 已完成
   - 新 5~6 区动态布局：标题栏 → 工作流进度条 → 对话区 → 权限栏(条件) → 状态提示栏 → 输入区
   - 精简标题栏为核心 3~4 项信息
   - 合并 Hints+Status 为 1 行上下文敏感状态提示栏
   - 权限栏按需显示（0~2 行）
   - 输入区去掉标题噪声
2. `P1-UX-2` 消息轮次模型
   - 引入 `ChatTurn`/`ToolCallCard` 数据模型，替代 `Vec<String>` 日志行
   - 用户消息 / 助手回复带视觉边框与轮次标识
   - 工具调用渲染为可折叠卡片 `▸/▾ name status duration`
   - 推理内容默认折叠
3. ~~`P1-UX-3` 样式与主题~~ ✅ 已完成
   - 引入 `TuiTheme` 语义化颜色变量（`text_strong/text_base/primary/success/danger` 等）
   - 所有渲染颜色经由主题间接引用，不再硬编码
   - 工作流进度条加 spinner 动画
   - 工具执行状态改为语义化文案（"Searching codebase..." 替代 "processing..."）
4. ~~`P1-UX-4` 交互增强~~ ✅ 已完成
   - ~~输入历史（↑/↓ 回溯）~~ ✅
   - ~~多行输入（Shift+Enter 换行）~~ ✅
   - 权限区独立交互（y/n/a 快捷键）— 延期：需 async channel 重构（当前权限确认走 stdin 阻塞）
   - ~~焦点管理分离（输入 vs 滚动）~~ ✅
   - ~~简单 Markdown 渲染（代码块高亮、列表缩进、标题加粗）~~ ✅
5. ~~`P1-UX-5` polish~~ ✅ 已完成
   - ~~Token 使用进度条~~ ✅
   - ~~长输出截断 + 展开提示~~ ✅
   - ~~首次启动引导简化~~ ✅
6. ~~`P1-UX-6` 过程展示体验优化（Process Display UX）~~ ✅ 已完成
   - ~~6a 三级详细度模型 (Compact/Normal/Verbose)~~ ✅
   - ~~6b 阶段切换去重与精简~~ ✅
   - ~~6c 工具调用单行概要~~ ✅
   - ~~6d Token 使用内联格式化~~ ✅
   - ~~6e 权限交互增强（PermBlock + PermHint）~~ ✅
   - ~~6f 轮次分组与视觉分隔~~ ✅
   - 设计背景与动机见下方 §P1-UX-6 详细规划

### P1-UX-6 详细规划：过程展示体验优化

**问题诊断**（基于当前 TUI 截图）：

当前对话面板中的过程事件呈现存在以下体验问题：

```
◆ [stage:planning]                                              ← 与下行重复
◇ [Workflow][r0] workflow_stage: planning | build_prompt...      ← 同一阶段两行
◆ [stage:executing]
◇ [Workflow][r1] workflow_stage: executing | llm_round_start
→ [Step][r1] llm_round_1_start                                  ← 内部实现细节
[Usage][r1] token_usage: source=provider prompt=31213 ...        ← 原始 k=v 转储
→ [Step][r1] llm_round_1_finish (11139ms)                       ← 内部实现细节
◆ [stage:discovery]
◇ [Workflow][r1] workflow_stage: discovery | tool_calls_planned
◌ [Thinking][r1]
└─ planning tool calls: shell({"command":"ls -la"...})           ← 原始 JSON
▸ [r1] start shell
└─ input : {"command":"ls -la","working_dir":"..."}              ← 原始 JSON 重复
⚠ [Permission][r1] permission_asked: Command not allowed: ls -la ← 无操作指引
✗ [r1] failed shell (0ms)                                       ← 与上行关联不明显
```

| # | 问题 | 影响 |
|---|------|------|
| 1 | 阶段变更重复显示 | `◆ [stage:X]` + `◇ [Workflow] workflow_stage: X` 两行表达相同语义 |
| 2 | 内部步骤暴露 | `llm_round_1_start/finish` 是实现细节，非用户关注 |
| 3 | Token 使用原始转储 | `token_usage: source=provider prompt=31213 completion=76...` 一长行 k=v |
| 4 | 工具输入为原始 JSON | `{"command":"ls -la","working_dir":"..."}` 对用户不友好 |
| 5 | 权限拒绝无行动指引 | 只显示 "not allowed"，没有提示如何解除 |
| 6 | 无视觉层次/分组 | 所有事件扁平排列，无法快速区分"对话轮次 > 工具调用 > 详情" |
| 7 | 单次工具调用产出 4~5 行 | start + input + permission + failed + output 全部展开，信息密度低 |

**设计目标**：

- **默认模式只展示用户关心的信息**：阶段切换、工具执行概要、结果/错误、权限提示。
- **内部实现细节按需展开**：Step/Workflow/Token 详情仅在 detail/debug 模式可见。
- **工具调用一行概要**：`▸ shell "ls -la" → ✗ Permission denied`，只在展开时才显示 JSON。
- **权限阻塞有明确操作指引**：提示用户可用的操作选项。
- **轮次分组有视觉边界**：相同 round 的事件视觉纳入一组。

**详细子任务**：

#### P1-UX-6a 三级详细度模型（Verbosity Tiers）

引入 `DisplayVerbosity { Compact, Normal, Verbose }` 枚举，控制 `event_to_lines()` 输出策略：

| 事件类型 | Compact（默认） | Normal（/detail） | Verbose（/debug） |
|----------|-----------------|-------------------|-------------------|
| WorkflowStage | `◆ planning` 单行 | + detail 描述 | + 原始 message |
| StepStart/Finish | 隐藏 | 仅 Finish 含耗时 | start + finish 全展 |
| TokenUsage | 隐藏（已在状态栏） | `tok 31.3k (+76)` 单行 | 原始 k=v |
| ToolCallStart | `▸ shell "ls -la"` | + 格式化参数 | + 原始 JSON args |
| ToolCallEnd | 合并到 start 行尾 | + output preview | + meta/call_id |
| Reasoning | 折叠提示 | 首行 + "..." | 全文 |
| PermissionAsked | 高亮 + 操作指引 | 同左 | + 原始消息体 |

切换方式：`Ctrl+D` 循环 Compact → Normal → Verbose，`/verbosity <level>` 直接设置。

#### P1-UX-6b 阶段切换去重与精简

`WorkflowStage` 事件合并为**单行语义摘要**，去掉冗余：

- 当前：
  ```
  ◆ [stage:planning]
  ◇ [Workflow][r0] workflow_stage: planning | build_prompt_and_context
  ```
- 目标（Compact）：
  ```
  ◆ Planning...
  ```
- 目标（Normal）：
  ```
  ◆ Planning — building context
  ```

#### P1-UX-6c 工具调用单行概要

将 ToolCallStart + ToolCallEnd 合并为**一行概要**（配合 Compact 模式）：

- 当前（5 行）：
  ```
  ▸ [r1] start shell
  └─ input : {"command":"ls -la","working_dir":"..."}
  ⚠ [Permission][r1] permission_asked: Command not allowed: ls -la
  ✗ [r1] failed shell (0ms)
  ```
- 目标 Compact（1~2 行）：
  ```
  ▸ shell "ls -la" ✗ permission denied
    └ Tip: /allow shell 或设置 NDC_TOOL_ALLOW=shell
  ```
- 目标 Normal（2~3 行）：
  ```
  ▸ shell: ls -la (dir: /home/.../ndc)
    ✗ Permission denied — command not in allow list
    └ Tip: /allow shell, or reply 'y' to allow this once
  ```

实现要点：
- `extract_tool_summary(tool_name, args_json) -> String`：从 JSON 参数提取人性化摘要（shell→command, read→path, write→path, grep→pattern）。
- ToolCallStart 在 Compact 模式下缓存到 `pending_tool_call`，等 ToolCallEnd 或 Permission 时一并输出。

#### P1-UX-6d Token 使用内联格式化

- 将原始 `token_usage: source=provider prompt=31213 completion=76 total=31289 | session_...` 替换为：
  - Compact：隐藏（已在状态栏进度条展示）。
  - Normal：`  tok +31.3k (31.3k total, 11.1s)` 单行精简。
  - Verbose：保留原始 k=v 供调试。

#### P1-UX-6e 权限交互增强

Permission 事件改为高可见度卡片样式：

```
┌ ⚠ Permission Required ─────────────────────┐
│ shell: ls -la                               │
│ Risk: Medium — command not in allow list     │
│                                             │
│ [y] allow once  [a] allow all  [n] deny     │
└─────────────────────────────────────────────┘
```

实现路径：
- 短期（当前 stdin 阻塞模式）：在 `style_session_log_line` 渲染为多行高亮块，附带操作提示文案。
- 长期（P1-UX-4 延期项）：async channel 重构后，权限确认走 TUI 事件循环，支持真正的 y/n/a 按键。

#### P1-UX-6f 轮次分组与视觉分隔

- 在 round 切换时插入轻量分隔线：`── Round 2 ──`（仅 Normal/Verbose 模式）。
- 同一 round 内事件统一缩进 2 格，形成视觉层次。
- Thinking 内容与工具调用在视觉上归属为子层级。

#### P1-UX-6 执行优先级

| 优先级 | 子任务 | 依赖 | 复杂度 |
|--------|--------|------|--------|
| 1 | P1-UX-6b 阶段去重 | 无 | 低 |
| 2 | P1-UX-6d Token 格式化 | 无 | 低 |
| 3 | P1-UX-6c 工具单行概要 | 无 | 中 |
| 4 | P1-UX-6e 权限增强 | 无 | 中 |
| 5 | P1-UX-6a 三级详细度 | 6b/6c/6d | 中 |
| 6 | P1-UX-6f 轮次分组 | 6a | 中 |

### P1 其他执行清单（P0-D 完成后推进）

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
3. ~~REPL 可视化进度与历史重放~~ → 已纳入 P1-UX

## 已完成里程碑（压缩）

- P0-A：REPL UI 对齐 OpenCode（固定输入区、可滚动 session、快捷键、命令提示补全）
- P0-B：多轮对话实时可视化（事件模型、timeline 回放、实时流、SSE/gRPC、脱敏）
- 工程治理：移除空 crate、storage 独立抽取、edition 2024 统一
- 主链能力：tool-calling、task verify、memory/invariant 回灌、storage 打通等主线修复

> 详细历史请查看 `git log` 与 `docs/plan/current_plan.md`，本文件仅保留“可执行待办 + 里程碑摘要”。

## 验收门禁（P0/P1 合并前）

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新（本文件 + 相关计划/用户文档）
