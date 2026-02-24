# NDC TODO / Backlog

> 更新时间：2026-02-24（晚）  
> 与 `docs/plan/current_plan.md` 对齐。

## 已完成（本轮修复）

- `run --message` 主链接入真实 Agent（非占位输出）
- 默认工具注册统一，支持 function-calling schema 透传
- OpenAI/OpenRouter tool-calling 请求协议打通
- `ndc_task_*` 从 mock 改为真实存储实现
- 工具执行与 TaskVerifier 使用同一份 runtime storage
- ReplToolExecutor 接入 `allow/ask/deny` 权限判定与交互确认
- Orchestrator 会话消息回写（用户/助手/工具）
- Discovery -> HardConstraints -> QualityGate 强制链路落地（执行阶段强制附加质量检查）
- WorkingMemory（Abstract/Raw/Hard）注入 Agent Prompt 主循环（增强系统提示词路径）
- 新增主链 smoke 测试：
  - `ndc_task_create -> ndc_task_update -> ndc_task_verify`
  - 文件工具调用 + 会话续接（Agent Orchestrator）
- Discovery 失败策略可配置：
  - `degrade`（默认，降级继续）
  - `block`（严格模式，阻断执行）
  - 支持 `runtime.discovery_failure_mode` 与 `NDC_DISCOVERY_FAILURE_MODE`
- WorkingMemory 注入升级为真实任务源：
  - 从活跃任务提取失败历史（Abstract）与当前文件/步骤（Raw）
  - 从质量门禁与 memory 访问记录提取约束文本注入（Hard-like constraints）
- 主链 smoke 扩展：
  - 覆盖 QualityGate 失败后的反馈续执行环路（orchestrator verification feedback loop）
  - 覆盖权限 `ask/deny` 分支（含 `NDC_AUTO_APPROVE_TOOLS` 自动确认）
- Hard Invariants 类型统一与结构化注入：
  - `WorkingMemory::VersionedInvariant.priority` 统一使用 Gold Memory `InvariantPriority`
  - `AgentMode` 已恢复结构化 Hard Invariants 注入（非纯文本拼接）
- Invariant Injector 优先级类型收敛完成：
  - `ai_agent/injectors/invariant.rs` 已复用 core `InvariantPriority`（去除第三套定义）
  - 修复 `InvariantInjector::default` 递归与 `get_active` 过滤逻辑
- Hard Invariants 回灌闭环落地：
  - `TaskVerifier::verify_and_track` 失败时固化 invariant（GoldMemory）
  - 同任务后续验证成功时累计 validated 计数
  - Orchestrator 自动验证路径已切换到 `verify_and_track`
- GoldMemory 持久化与会话复用打通：
  - `TaskStorage` 增加 memory 读写抽象，Verifier 可直接持久化 GoldMemory
  - GoldMemoryService 序列化入 runtime storage 并在新 verifier 实例自动加载
  - AgentMode 默认使用该持久化回灌链路
- Discovery/QualityGate 结构化事实映射接入：
  - `TaskVerifier` 将 `VerificationResult` 映射为结构化 rule/tags/evidence 写入 GoldMemory
  - 对重复失败按 task+failure_key 去重，并累计 `violation_count`
  - 质量门禁失败映射为 `Critical` 优先级事实
- GoldMemory schema 版本化与兼容迁移：
  - 持久化格式升级为 `gold_memory_service/v2` 包装载荷
  - 保持对 `gold_memory_service/v1` 历史数据读取兼容
  - 在回灌写入路径自动完成 `v1 -> v2` 迁移
- Discovery 执行阶段结构化信号回灌：
  - `Executor` 在 discovery 失败或产出 hard constraints 时写入结构化 system facts
  - 统一复用 GoldMemory 持久化 entry（`gold_memory_service/v2`）
  - 覆盖 e2e 验证（含并发环境变量隔离锁）
- GoldMemory 迁移审计元数据增强：
  - `v2` 载荷新增 `migration` 审计块（`from_version/migrated_at/trigger_task_id/trigger_source`）
  - verifier 与 executor 均写入统一审计字段
- `v1 -> v2` 迁移测试已覆盖审计字段断言
- Discovery/Verifier 双源事实统一去重与冲突合并策略已落地：
  - `GoldMemoryService::upsert_system_fact` 作为统一规则引擎
  - 统一 `dedupe_key`，重复事实走合并并升级优先级/证据聚合
- GoldMemory 事实检索工具已接入：
  - 新增 `ndc_memory_query`（按 `tags/priority/source` 查询）
  - 默认工具管理器与 tool registry 均已注册
  - e2e smoke 已覆盖查询链路
- P0 第一批可视化能力已落地：
  - 新增统一执行事件模型：`step/tool/reasoning/text/verification/session_status/error`
  - `AgentResponse` 增加 `execution_events` 回传，orchestrator 主循环已记录轮次与耗时
  - 会话对象已支持保存执行事件时间线（内存态）
  - REPL 新增 `/thinking`、`/details`、`/timeline [N]`
  - REPL 已支持工具开始/结束/失败与耗时展示，支持时间线回看
  - 新增测试：
    - orchestrator smoke 断言 `ToolCallStart/ToolCallEnd`
    - repl 时间线容量与可视化状态测试
  - `docs/USER_GUIDE.md` 已新增多轮可视化使用说明
- P0 会话时间线回放接口（第二批）已落地：
  - `AgentOrchestrator::get_session_execution_events(session_id, limit)` 已实现
  - `AgentModeManager::session_timeline(limit)` 已实现（无会话时返回空）
  - REPL `/timeline` 已切换为优先读取会话时间线（支持回放视图）
  - orchestrator smoke 已增加时间线回放断言
- P0 对外接口（第三批）已落地（拉取模式）：
  - gRPC `AgentService.GetSessionTimeline` 已实现
  - 对外返回标准化 `ExecutionEvent`（kind/timestamp/round/tool/duration/error）
  - 新增 gRPC 事件映射单测
- P0 对外接口（第四批）已落地（流式模式，事件推送 + 轮询补偿）：
  - gRPC `AgentService.SubscribeSessionTimeline` 已实现
  - 基于 orchestrator 事件总线实时推送 `ExecutionEvent`（服务端流）
  - 保留按时间线轮询补偿（处理 lag/丢帧/重连窗口）
  - 与 `GetSessionTimeline` 使用同一事件模型
- P0 客户端 SDK 接入（第五批）已落地：
  - `grpc_client` 已新增 `get_session_timeline` / `subscribe_session_timeline`
  - 新增 timeline request 构造单测
- P0 对外接口（第六批）已落地（SSE 流式）：
  - 新增 SSE 订阅接口：`GET /agent/session_timeline/subscribe?session_id=&limit=`
  - SSE 与 gRPC 共用同一会话时间线与事件映射模型
  - `grpc_client` 新增 `timeline_sse_subscribe_url(session_id, limit)` 便于外部 EventSource 接入
  - 支持 `NDC_TIMELINE_SSE_ADDR=<host:port|auto>` 与 `NDC_TIMELINE_SSE_POLL_MS`
  - 已新增 SSE 集成测试：订阅 `200 + text/event-stream` 与非法 `session_id` 返回 `404`
  - 已新增 SSE 回放事件测试：可回放 `execution_event` 且包含标准化 `kind` 字段
- P0 安全与隐私策略（第一批）已落地：
  - REPL 时间线/事件输出默认脱敏（`api_key/token/password/Bearer/sk-*`、用户 home 路径）
  - gRPC `ExecutionEvent.message` 对外输出前默认脱敏
  - 新增 REPL 与 gRPC 脱敏单测
- P0 安全与隐私策略（第二批）已落地：
  - 脱敏逻辑已统一到 `interface::redaction` 模块（REPL + gRPC 共用）
  - 支持 `NDC_TIMELINE_REDACTION=off|basic|strict` 配置
  - strict 模式新增绝对路径脱敏
- P0 可视化状态事件与默认开关（第六批）已落地：
  - 执行事件模型新增 `PermissionAsked`，用于标记权限询问/拒绝分支
  - REPL 可显式展示 `Permission` 事件（便于识别等待授权状态）
  - 支持 REPL 可视化默认配置：
    - `NDC_DISPLAY_THINKING=true|false`
    - `NDC_TOOL_DETAILS=true|false`
    - `NDC_TIMELINE_LIMIT=<N>`
  - 新增测试：orchestrator 权限事件断言、REPL 可视化环境变量断言
  - 新增多轮回放断言：会话 timeline 中可检索 `PermissionAsked` 事件
- P0 交互可用性（第七批）已落地：
  - `thinking` 默认折叠（降低默认噪音）
  - 新增快捷别名：`/t`（thinking）、`/d`（details）
  - 新增 `/thinking show` 可在折叠模式下即时查看最近思考
  - 折叠状态下收到 reasoning 事件会显示可见提示
- P0 REPL 实时流（第八批）已落地：
  - REPL TUI 已从 `session_timeline` 轮询切换为 orchestrator 广播订阅（实时推送）
  - 若实时流不可用/关闭，会自动回退到原有时间线轮询路径
  - 新增测试覆盖：
    - core: `subscribe_execution_events` 广播会话事件
    - interface: AgentMode 订阅接口 + REPL live drain 渲染

## P0-A（最高优先级：REPL UI 对齐 OpenCode，先解决未完成功能）

> 目标：参考 `opencode/packages/opencode/src/cli/cmd/tui/routes/session/index.tsx` 与相关 keybind/transcript 设计，完成 REPL 终端 UI 改造。  
> 核心诉求：固定输入窗口、消息区/状态区分离、折叠/展开交互一致、可观测性更强。

当前主项状态（P0-A）：

1. 固定输入窗口（输入区始终停靠底部，不随日志滚动）【已完成】
2. 消息流滚动区与输入区解耦（上方滚动，下方输入）【已完成】
3. thinking 折叠/展开的“就地查看”交互（默认折叠，快捷键展开）【已完成】
4. tool 结果详情的折叠卡片化展示（状态、输入、输出、错误分区）【已完成】
5. 会话级状态栏（provider/model/session/开关状态）常驻显示【已完成】
6. 键盘交互统一（最少包含 thinking/details/timeline/clear 的快捷切换）【已完成】
7. Session 面板可滚动与分层样式化展示（参考 opencode transcript/tool 行样式）【已完成】
8. 实时流可观测与开关（状态栏+命令）【已完成】
9. 输入命令提示与补全（`/` 触发提示，Tab/Shift+Tab 浏览全部候选，含参数选项）【已完成】

实施项：

1. REPL 终端布局重构【已完成】
   - 引入固定 bottom prompt 区域与可滚动 transcript 区域
   - 输出渲染改为“消息块”而非纯 println 流
   - 第一阶段已完成：默认启用 TUI 布局（状态栏 + 滚动会话区 + 固定输入区）
   - 保留 `NDC_REPL_LEGACY=1` 回退路径（便于兼容排障）
   - 第四阶段优先：Session 面板滚动（Up/Down/PgUp/PgDn/Home/End/鼠标滚轮）【已完成】
2. 交互与快捷键【已完成】
   - 参考 opencode keybind 思路，增加可配置键位映射
   - 支持命令别名与快捷键双通道（避免只靠 slash 命令）
   - 第二阶段已完成：TUI 快捷键已接入（`Ctrl+T/Ctrl+D/Ctrl+E/Ctrl+Y/Ctrl+I/Ctrl+L`）
   - 第二阶段已完成：支持环境变量覆盖快捷键（`NDC_REPL_KEY_*`）
   - 命令别名与快捷键已统一（`/t`、`/d` + Ctrl 快捷键）
   - 第三阶段补强：会话 timeline 快捷查看（`Ctrl+I`，可配置 `NDC_REPL_KEY_SHOW_TIMELINE`）
3. 内容分层渲染【已完成】
   - Thinking block（默认折叠）/ Tool block / Text block 统一样式层级
   - 支持按 block 展开最近内容，而不是全局开关硬切
   - 第二阶段已完成：消息区新增 `You:` / `Assistant:` 分层块输出
   - 已支持折叠态快捷展开最近 thinking（`Ctrl+Y` 或 `/thinking show`）
   - 第三阶段已完成：Tool/Thinking 输出改为块样式（input/output/meta 分层）
   - Tool 事件已补充 `args_preview` 与 `result_preview` 双预览
   - 第三阶段补强：新增 tool 卡片展开/折叠开关（`/cards`、`Ctrl+E`、`NDC_TOOL_CARDS_EXPANDED`）
   - 第三阶段补强：tool 卡片按 `input/output/error/meta` 分区展示
   - 第三阶段补强：状态栏常驻会话信息（provider/model/session/开关态）
   - 第四阶段优先：Session 行级样式化（tool/thinking/error/input/output/meta 区分色彩与层次）【已完成】
4. 验收与测试【已完成】
   - 已新增 UI 行为测试：固定输入区布局约束、滚动计算、快捷键解析
   - 已新增可视化日志测试：timeline 快捷展示与状态栏字段断言
   - 已新增快照式单测：折叠态/展开态/tool 卡片展开态渲染断言
   - 已补齐交互级测试：运行态快捷键触发（Ctrl+T/Ctrl+L）与滚动复位行为
   - 已补齐滚动与样式测试：键盘滚动、鼠标滚轮、行级样式渲染断言
   - 已补齐实时流状态测试：`stream=off|ready|live|poll` 状态映射与 `/stream` 命令行为断言
   - 已补齐输入提示与补全测试：`/` 命令提示渲染、Tab/Shift+Tab 循环补全与 `/provider` 参数候选

## P0-B（次高优先级：多轮对话可视化与实时状态）

> 目标：在多轮对话中实时可见 AI “正在做什么”、做到了哪一步、调用了哪些工具、为什么停下/等待输入。  
> 参考：`opencode` 的 `thinking` 显示、`tool_details`、`session_timeline`、`event.subscribe()` 事件流。

1. 统一事件模型（Orchestrator/REPL/gRPC）【进行中】
   - 扩展并统一事件类型：`step_start` / `step_finish` / `tool_call_start` / `tool_call_end` / `reasoning` / `text` / `permission_asked` / `session_status` / `error`
   - `permission_asked` 已落地（权限拒绝/询问分支）
   - 明确每类事件的字段（`session_id`、`message_id`、`tool_call_id`、时间戳、耗时、摘要）
   - 约束顺序与幂等语义，保证多轮与重试场景可回放
2. REPL 实时渲染（默认可读、细节可切换）【第一批完成】
   - 新增 `"/thinking"`：切换 reasoning 显示（默认关闭，避免噪音）
   - 新增 `"/details"`：切换工具调用详细参数/结果显示
   - 新增 `"/timeline"`：查看当前会话步骤时间线（最近 N 条）
   - 对每次工具调用输出“开始/完成/失败 + 耗时”，并与最终回答分区展示
   - REPL 已改为事件订阅推送（非轮询），终端实时性提升
   - 新增 `/stream [on|off|status]`，可在会话内切换实时广播/轮询回退
   - 状态栏新增 `stream=off|ready|live|poll`，可实时观察当前流模式
3. 多轮会话时间线持久化与重放【进行中】
   - 将事件写入会话存储（最小必要字段 + 可裁剪 payload）【已完成：内存会话态】
   - 提供会话重放接口：按时间/轮次恢复执行轨迹【已完成：按 session + limit】
   - 为后续 GUI 与 WebSocket/SSE 订阅提供统一数据源
4. 对外流式接口补齐（CLI/SDK 友好）【第二批完成】
   - 在现有接口上提供稳定的流式事件订阅能力（与 REPL 同一事件源）【已完成：gRPC stream + SSE】
   - gRPC 订阅语义补强：`limit` 支持初始 backlog 回放（`0`=仅新事件，`>0`=先回放后增量）
   - gRPC 实时推送已接入；轮询补偿间隔可配置：`NDC_TIMELINE_STREAM_POLL_MS`（50..2000ms）
   - SSE 订阅接口：`GET /agent/session_timeline/subscribe?session_id=&limit=`（`execution_event`/`error`）
   - SSE 服务地址配置：`NDC_TIMELINE_SSE_ADDR=<host:port|auto>`（`auto` = gRPC 端口 + 1）
   - SSE 实时推送已接入；轮询补偿间隔可配置：`NDC_TIMELINE_SSE_POLL_MS`（50..2000ms）
   - SDK 增加 SSE 订阅 URL 构造：`timeline_sse_subscribe_url(session_id, limit)`
   - 文档化事件协议，确保外部前端可实时展示 agent 执行状态【已完成：拉取 + gRPC + SSE】
5. 安全与隐私策略【进行中】
   - 对 reasoning/tool 参数做脱敏策略（路径、密钥、token、隐私内容）【第二批完成：统一模块 + 分级策略】
   - 提供配置项：`display_thinking`、`tool_details`、`timeline_limit`【已完成（环境变量）】
6. 验收与测试（必须）【进行中】
   - 新增 e2e：多轮 + 多次 tool call + 权限询问 + 中断恢复 + 时间线回放【部分完成：core 多轮 + 权限询问 + 回放】
   - SSE 接口集成测试：`/agent/session_timeline/subscribe`（成功握手 + 会话校验）【已完成：interface/grpc tests】
   - SSE 回放事件内容测试：验证 `execution_event` 载荷包含 `kind`（`SessionStatus/StepStart`）【已完成：interface/grpc tests】
   - REPL 快照测试：`thinking/details/timeline` 三种开关组合【已补齐：interface 单测覆盖】
   - 文档更新：`docs/USER_GUIDE.md` 增加“如何实时观察 AI 执行过程”

## P1（高优先级）

1. GoldMemory 检索结果接入 orchestrator 自动上下文选择（按任务上下文注入 Top-K facts）
2. Failure Taxonomy 接入重试与回滚策略（含 NonDeterministic）
3. Invariant 的 TTL/version/conflict 检查接入执行前阶段
4. Telemetry 指标落地（autonomous_rate / intervention_cost / token_efficiency）
5. MCP/Skills 接入默认工具发现链与权限治理链

## P2（后续增强）

1. 多 Agent 协同编排（planner / implementer / reviewer）
2. 文档自动回灌与知识库固化策略（阶段 8）
3. REPL 可视化进度与历史重放

## 验收门禁

每个 P0/P1 任务合并前必须满足：

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新（本文件 + 架构重规划）
