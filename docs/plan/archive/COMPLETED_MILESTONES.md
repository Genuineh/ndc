# NDC 已完成里程碑归档

> 从 `docs/TODO.md` 归档于 2026-02-27  
> 此文件记录已完成功能的详细实现记录，供回溯参考。活跃待办请见 `docs/TODO.md`。

---

## P0-A（已完成：REPL UI 对齐 OpenCode）

- 固定输入区、可滚动 session、快捷键、命令提示补全。

## P0-B（已完成：多轮对话实时可视化）

- 事件模型、timeline 回放、实时流、SSE/gRPC、脱敏。

## P0-C（已完成：Workflow-Native REPL 与实时可观测）

目标：终端实时看到 `当前 workflow 阶段 + 阶段进度 + 本轮/累计 token + 工具耗时/错误分布`。REPL / gRPC / SSE 使用同一套观测语义。

实现记录：

- `core` 已固化 `AgentWorkflowStage` 作为阶段真相源（planning/discovery/executing/verifying/completing）
- orchestrator 主循环已发射阶段事件并覆盖多轮
- `AgentExecutionEvent` 已补齐结构化 workflow 字段：`workflow_stage/workflow_detail/workflow_stage_index/workflow_stage_total`
- REPL / gRPC 已优先消费结构化 workflow 字段（message 仅作人类可读回退）
- token usage 已接入（provider 优先，缺失回退 estimate）
- REPL 状态栏已展示：`workflow`、`workflow_progress`、`workflow_ms`、`blocked`、`tok_round/tok_session`
- REPL `/workflow`、`/tokens`、`/metrics` 已可用
- REPL `/workflow` 已支持 `compact|verbose` 双视图
- workflow 阶段耗时统计已补边界（active_ms 累计、历史缓存上限提示）
- Session 面板与 `/timeline` 已按 `[stage:<name>]` 分段展示
- gRPC/SSE `ExecutionEvent` 已扩展 workflow 与 token 字段
- 兼容策略文档已补齐（旧客户端忽略新增字段可降级）
- 已新增综合 e2e + interface 侧结构化字段测试 + 订阅重放一致性 e2e

## P0-D（已完成：安全边界与项目级会话隔离）

设计导航：`docs/design/p0-d-security-project-session.md`

目标：
- 危险操作可控：对 shell/fs/git 的高风险行为进行统一判定、确认与拒绝。
- 项目上下文隔离：不同项目（A/B）必须拥有独立 session 上下文、历史和 resume 入口。
- resume 正确归属：在项目 A 中 continue/resume 只能回到 A 的会话，不串到其他项目。
- 与 OpenCode 对齐：参考 `opencode/specs/project.md` 等设计语义。

### P0-D1 项目身份识别与上下文归属

- `core` 新增 `ProjectIdentity` 解析（git root commit / non-git path fingerprint）。
- `AgentSession` 新增项目元数据字段：`project_id/project_root/working_dir/worktree`。
- orchestrator 创建新会话时已注入项目身份。
- REPL 状态栏与 `/status` 已展示 `project` 标识。

### P0-D2 项目级 session 索引与 resume

- orchestrator 新增项目会话索引与最近会话游标。
- `AgentModeManager` 新增会话控制 API：`start_new_session`、`resume_latest_project_session`、`use_session`。
- REPL 新增 `/new`、`/resume`（支持 `/resume <id> [--cross]`），并接入 hints/补全。
- CLI `run` 已接入 `--continue`、`--session <id>`、`--allow-cross-project-session`。
- gRPC/SSE 已接入 `AgentModeManager::use_session`（同项目可切换，跨项目默认拒绝）。
- 新增回归测试：跨项目会话拒绝、项目最近会话游标、REPL `/resume` 参数补全。

### P0-D1/D2 后续批次

- REPL 启动时显示当前项目 + 附近项目列表 + 当前项目最近 session。
- `/project` 命令族：`status / list / use <index|path> / sessions [project-id]`。
- `Ctrl+P` 快捷键直接打开项目选择器。
- 持久化项目索引：`~/.config/ndc/project_index.json`，跨进程恢复已知项目。
- 持久化会话归档：`~/.config/ndc/session_archive.json`，启动时自动恢复最近会话。

### P0-D3 危险操作统一权限网关

- runtime 新增 `tools::security` 网关：
  - `external_directory` 边界判定（默认 `ask`）
  - shell 风险分级（`Critical` deny、`High` ask、`Medium` 可配置）
  - git `commit` 默认 `ask`
- 已接入工具主链：`shell/fs/git/read/write/edit/list/glob/grep`。
- REPL `ReplToolExecutor` 确认闭环：收到 `requires_confirmation` 后可确认并重试。
- 单次授权覆盖机制：`with_security_overrides(...)`。
- 项目切换后工具上下文生效：`ShellTool` 注入 `working_dir`，`PromptBuilder` 注入项目上下文。

### P0-D4 REPL 实时安全可观测

- 状态栏新增 `perm_state/perm_type/perm_risk` 字段。
- Permission 事件行新增结构化标签。
- orchestrator 已补齐权限事件链：`permission_asked -> permission_approved/rejected -> tool_call_end`。

### P0-D5 测试与回归

- gRPC 映射/序列化回归：`permission_asked/approved/rejected` 一致性。
- 订阅重放一致性校验。
- 跨 manager/重启场景持久化权限事件链回归。
- SSE 回放与字段语义一致性。
- SSE 测试连接抖动修复（短时重试）。

### P0-D6 文档与配置

- 非交互通道（no TTY）返回结构化拒绝。
- 迁移文档：`docs/design/p0-d6-non-interactive-migration.md`。
- `USER_GUIDE` 新增 6.2（非交互通道行为）。
- runtime 网关默认启用/测试模式默认值/CSV 覆盖解析 回归测试。

---

## P1-UX（已完成：REPL TUI 布局与体验重设计）

目标：从"日志行"体验升级为"对话轮次"体验，对齐 OpenCode 等现代 AI 编码助手的交互风格。

### P1-UX-1 布局重构 ✅

- 新 5~6 区动态布局：标题栏(1) → 工作流进度条(1) → 对话区(Min5) → 权限栏(条件2) → 状态提示栏(1) → 输入区(动态3~6)。
- `tui_layout_constraints(has_permission, input_lines)` 动态返回约束向量。
- `build_title_bar()` 精简为：品牌标识、项目名、会话 ID、模型、状态。
- `build_workflow_progress_bar()` 五阶段 pipeline。
- `build_permission_bar()` ⚠ 图标 + 操作提示（y/n/a）。
- `build_status_hint_bar()` 合并旧 Hints+Status 为 1 行上下文敏感提示。

### P1-UX-3 主题系统 ✅

- `TuiTheme` 20 个语义化颜色变量 + `default_dark()` 深色方案。
- `style_session_log_line()` 全部颜色改为 theme 引用。
- 视觉图标：`▌ ◆ ✗ ✓ ▸ ◇ ◌ →`。
- `tool_status_narrative()` 按工具类型输出语义化文案。

### P1-UX-4 交互增强 ✅

- `InputHistory`（去重、容量 100、草稿保存、↑/↓ 导航）。
- 多行输入（Shift+Enter / Alt+Enter，动态扩展至 4 行）。
- 焦点分离：↑/↓ = 历史，Ctrl+↑/↓ = 滚动对话。
- 权限消息提取与生命周期管理。
- 基础 Markdown 渲染：# 标题、- 列表（→•）、代码围栏、`code`、**粗体**、*斜体*。
- 延期项：权限区独立交互（y/n/a 快捷键）— 需 async channel 重构。

### P1-UX-5 Polish ✅

- Token 进度条：`format_token_count()` + 8 字符 `[████░░░░]`。
- 长输出截断（200 字符 + `… truncated`）。
- 启动信息精简单行。

### P1-UX-6 过程展示体验优化 ✅

- `DisplayVerbosity { Compact, Normal, Verbose }` 三级详细度模型。
- `event_to_lines()` 完全重写：按 verbosity 差异化输出。
- 阶段切换去重：Compact `◆ Planning...` / Normal 附 detail / Verbose 原始双行。
- `extract_tool_summary()` 从 JSON 参数提取人性化摘要。
- Token 分级：Compact 隐藏、Normal `tok +N (M total)`、Verbose 原始。
- 权限增强：`[PermBlock]` + `[PermHint]` 操作指引。
- 轮次分组：`── Round N ──` 分隔线（Normal/Verbose）。
- `style_session_log_line()` 6 种新行前缀样式。
- `format_duration_ms()` 人性化时长。
- `/verbosity` 命令 + `Ctrl+D` 循环。

#### P1-UX-6 原始设计文档

<details>
<summary>问题诊断与设计目标（点击展开）</summary>

**问题诊断**（基于旧 TUI 截图）：

| # | 问题 | 影响 |
|---|------|------|
| 1 | 阶段变更重复显示 | `◆ [stage:X]` + `◇ [Workflow] workflow_stage: X` 两行表达相同语义 |
| 2 | 内部步骤暴露 | `llm_round_1_start/finish` 是实现细节 |
| 3 | Token 使用原始转储 | 一长行 k=v |
| 4 | 工具输入为原始 JSON | 对用户不友好 |
| 5 | 权限拒绝无行动指引 | 只显示 "not allowed" |
| 6 | 无视觉层次/分组 | 所有事件扁平排列 |
| 7 | 单次工具调用产出 4~5 行 | 信息密度低 |

**设计目标**：默认模式只展示用户关心的信息，内部细节按需展开，工具调用一行概要，权限阻塞有操作指引，轮次分组有视觉边界。

**Verbosity 矩阵**：

| 事件类型 | Compact（默认） | Normal | Verbose |
|----------|-----------------|--------|---------|
| WorkflowStage | 单行 | + detail | + 原始 message |
| StepStart/Finish | 隐藏 | 仅 Finish 含耗时 | 全展 |
| TokenUsage | 隐藏 | 单行精简 | 原始 k=v |
| ToolCallStart | 单行摘要 | + 格式化参数 | + 原始 JSON |
| ToolCallEnd | 状态+耗时 | + output preview | + meta/call_id |
| Reasoning | 折叠提示 | 首行 | 全文 |
| PermissionAsked | 高亮+指引 | 同左 | + 原始消息体 |

</details>

### P1-UX 测试覆盖

- 116 个 repl 测试通过（含 21 个 P1-UX-6 新增 + 15 个适配）。
- 覆盖范围：`DisplayVerbosity` parse/cycle/label、`capitalize_stage`、`format_duration_ms`、`extract_tool_summary`（shell/read/grep/unknown）、verbosity 分级行为、`input_line_count`、动态 `tui_layout_constraints`、`format_token_count`、`token_progress_bar`、`truncate_output`（含 Unicode 边界）、`parse_inline_spans`、`render_inline_markdown`、输出截断、InputHistory、权限消息生命周期。

---

## 工程治理（2026-02-25）

- 移除 8 个空占位 crate（cli, context, daemon, execution, observability, plugins, repl, task）。
- 从 runtime 抽取独立 `ndc-storage` crate。
- 全 workspace 统一 Rust edition 2024。
- edition 2024 match ergonomics 与 unsafe env var 修复。

## 杂项修复（2026-02-25）

- MiniMax 别名 provider 配置键归一化回退。
- 任务状态机约束修复（禁止非法强制迁移）。
- tool 链 smoke 改为合法状态链。
- `Executor` 执行 intent 步骤结果回写修复。
- 安全边界判定 `working_dir/project_root` 提示避免误判。
- `USER_GUIDE` 修正会话订阅语义。

## P0-SEC（已完成：深度安全审计修复，2026-02-26）

全项目深度审计（52,505 LOC / 665 tests），修复 20+ 安全/健壮性/架构问题，新增 80+ 测试。

详细修复清单见 `docs/TODO.md` P0-SEC 章节。

主要修复分类：
- **Immediate**（6 项）：Shell 超时失效、路径遍历绕过、API Key 泄露、权限默认放行、SSRF 防护、环境变量控制
- **Short**（7 项）：Session 竞态、存储容量限制、工具输出注入、gRPC 并发限制、文件原子写、验证 panic、事件丢弃、LSP 超时、Session ID 校验
- **Medium**（4 项）：Config 范围校验、Storage Mutex 替换、SQLite 连接池、消息历史限制、unwrap 清理、文件大小限制
- **Structural**（4 项）：God Object 拆分（orchestrator/agent_mode/repl 三大模块）、管线缺口评估、死代码清理、CI 工作流、关键路径测试补充

## BugFix（2026-02-27）

- Shell 执行命令修复 `ca066da`：完整命令字符串（如 `"echo test"`）通过 `sh -c` 委托执行，支持管道/重定向。
- Ctrl+C 中断运行任务 `4ac083c`：处理中按 Ctrl+C 中断当前任务而非退出 REPL；新增 `AgentError::Cancelled`；状态栏动态提示。
