# NDC 用户指南

本文档基于当前代码实现（2026-02-24）整理，重点说明可直接使用的交互方式。

## 1. 安装与运行

```bash
# 构建
cargo build

# 查看命令
cargo run -- --help
```

## 2. 交互模式

### 2.1 单轮模式

```bash
cargo run -- run --message "请分析当前项目结构并提出改进建议"
```

说明：

- `run --message` 会启动 Agent 并执行一次请求
- Agent 会使用默认工具集（文件读写/搜索/Shell/Git 等）

### 2.2 REPL 模式

```bash
cargo run -- repl
```

进入后可以直接输入自然语言，例如：

- `请先阅读 README 和 docs，然后总结当前架构问题`
- `请定位 workflow 模块中的潜在 bug 并修复`
- `请跑测试并解释失败原因`

可用命令：

- `/help`
- `/provider`（或 `/providers`，切换供应商）
- `/model <provider>[/<model>]`
- `/status`
- `/thinking`（切换显示思考/推理摘要）
- `/t`（`/thinking` 快捷别名）
- `/thinking show`（在折叠模式下立即查看最近 thinking）
- `/details`（切换显示工具执行细节与步骤耗时）
- `/d`（`/details` 快捷别名）
- `/cards`（切换 tool 卡片展开/折叠）
- `/stream [on|off|status]`（切换/查看实时事件流；关闭后仅轮询）
- `/timeline [N]`（查看最近 N 条执行时间线）
- `/agent`
- `/clear`
- `exit`
- 输入以 `/` 开头时，Hints 面板会实时显示命令提示；按 `Tab`/`Shift+Tab` 可循环补全并遍历全部候选（提示会显示 `Selected [k/N]`）
- 参数提示已接入（例如输入 `/provider ` 会显示所有 provider 选项并可直接 Tab 选择）

TUI 快捷键（默认 REPL）：

- `Ctrl+T`：切换 thinking 显示
- `Ctrl+D`：切换 tool details 显示
- `Ctrl+E`：切换 tool 卡片展开/折叠
- `Ctrl+Y`：即时查看最近 thinking（不改变折叠状态）
- `Ctrl+I`：即时查看最近 timeline
- `Ctrl+L`：清空会话面板
- `Up/Down`：按行滚动 Session 面板
- `PgUp/PgDn`：按半页滚动 Session 面板
- `Home/End`：跳转到 Session 顶部/底部
- 鼠标滚轮：滚动 Session 面板
- 状态栏会显示 `scroll=follow|manual`，用于指示是否自动跟随最新日志
- 状态栏会显示 `stream=off|ready|live|poll`：
  - `off`：实时流关闭，仅轮询
  - `ready`：实时流开启，当前空闲
  - `live`：实时流开启，正在接收广播事件
  - `poll`：实时流不可用，已回退轮询
- `Esc`：退出 REPL
- 若需回退旧行式 REPL：`NDC_REPL_LEGACY=1 ndc repl`
- 快捷键可通过环境变量覆盖：
  - `NDC_REPL_KEY_TOGGLE_THINKING`（默认 `t`）
  - `NDC_REPL_KEY_TOGGLE_DETAILS`（默认 `d`）
  - `NDC_REPL_KEY_TOGGLE_TOOL_CARDS`（默认 `e`）
  - `NDC_REPL_KEY_SHOW_RECENT_THINKING`（默认 `y`）
  - `NDC_REPL_KEY_SHOW_TIMELINE`（默认 `i`）
  - `NDC_REPL_KEY_CLEAR_PANEL`（默认 `l`）
  - `NDC_TOOL_CARDS_EXPANDED=true|false`（默认折叠）
  - `NDC_REPL_LIVE_EVENTS=true|false`（默认 `true`）

### 2.3 多轮对话可视化（P0）

为了在多轮对话中实时看见 AI 在做什么，REPL 已支持以下可视化能力：

1. 工具执行可视化
   - 每次工具调用会显示 `start` / `done|failed`，并展示耗时（ms）
   - REPL TUI 默认使用实时事件订阅推送（若不可用自动回退为轮询）
2. 思考可视化（可开关）
   - 使用 `/thinking` 显示/隐藏模型的推理摘要（如果模型返回）
   - 默认折叠（避免噪音），折叠时会提示 `use /t or /thinking show`
3. 执行细节可视化（可开关）
   - 使用 `/details` 显示 LLM 轮次开始/结束、验证步骤等细节事件
   - 权限询问会显示为 `Permission` 事件，便于识别 AI 在等待人工授权
   - TUI 中 tool 事件采用分层块样式：`input / output / error / meta`
   - Session 区域中不同事件会使用分层样式（`tool/thinking/step/error/permission`）提升可读性
   - 使用 `/cards`（或 `Ctrl+E`）控制 tool 卡片展开/折叠
4. 时间线回看
   - 使用 `/timeline` 或 `/timeline 80` 查看当前会话内最近事件轨迹
   - `/timeline` 会优先从 Agent 会话时间线读取（不是仅看当前屏幕输出）
   - gRPC 侧可通过 `AgentService.GetSessionTimeline` 拉取同一时间线（供外部 UI/SDK 复用）
   - gRPC 侧可通过 `AgentService.SubscribeSessionTimeline` 订阅时间线增量事件（流式）
     - 订阅默认走实时事件推送；并带轮询补偿以覆盖 lag/短时中断窗口
   - SSE 侧可通过 `GET /agent/session_timeline/subscribe?session_id=<id>&limit=<N>` 订阅同一时间线（`text/event-stream`）
     - 事件类型：`execution_event`（正常事件）、`error`（流错误）
     - 事件数据为 JSON，字段与 gRPC `ExecutionEvent` 对齐（`kind/timestamp/message/round/tool_name/tool_call_id/duration_ms/is_error`）
     - 若 `session_id` 指向当前 daemon 非活跃会话，将返回 `404`
     - `limit>0` 时会先回放历史事件，再推送增量；回放事件同样使用 `execution_event` 类型
   - 订阅接口的 `limit` 语义：
     - `limit=0`：仅订阅“从现在开始”的新事件（不回放历史）
     - `limit>0`：先回放最近 N 条，再持续订阅增量
   - `grpc_client` SDK 已提供：
     - `get_session_timeline(session_id, limit)`
     - `subscribe_session_timeline(session_id, limit)`
     - `timeline_sse_subscribe_url(session_id, limit)`（返回 SSE URL，便于浏览器/EventSource 客户端接入）
   - 可通过 `NDC_TIMELINE_STREAM_POLL_MS` 调整 gRPC 轮询补偿间隔（默认 `200`，范围 `50..2000` ms）
   - 可通过 `NDC_TIMELINE_SSE_POLL_MS` 调整 SSE 轮询补偿间隔（默认跟随 `NDC_TIMELINE_STREAM_POLL_MS`）
   - 可通过 `NDC_TIMELINE_SSE_ADDR` 启用 SSE 监听地址（例如 `127.0.0.1:4097`；`auto` 表示 gRPC 端口 + 1）
   - 默认会对可视化事件做脱敏显示（如 `api_key/token/password`、`Bearer`、`sk-...`、`/home/<user>`）
   - 可通过环境变量调整脱敏强度：
     - `NDC_TIMELINE_REDACTION=off`：关闭脱敏
     - `NDC_TIMELINE_REDACTION=basic`：默认规则（推荐）
     - `NDC_TIMELINE_REDACTION=strict`：更激进，额外脱敏绝对路径
   - 可通过环境变量设置默认可视化开关：
     - `NDC_DISPLAY_THINKING=true|false`
     - `NDC_TOOL_DETAILS=true|false`
     - `NDC_TIMELINE_LIMIT=<N>`

推荐组合：

- 初次排障：`/details` 打开，`/thinking` 关闭
- 需要理解策略：`/thinking` + `/details` 都打开
- 复盘多轮执行：使用 `/timeline 100`

对外订阅（gRPC + SSE）示例：

```bash
NDC_TIMELINE_SSE_ADDR=auto cargo run --features grpc -- daemon --address 127.0.0.1:4096
```

## 3. 可用 CLI 命令

```bash
ndc run --message "..."
ndc run
ndc repl
ndc daemon
ndc search <query>
ndc status-system
```

## 4. LLM 配置

### 4.1 环境变量

推荐使用 `NDC_` 前缀：

```bash
export NDC_OPENAI_API_KEY="..."
export NDC_OPENROUTER_API_KEY="..."
export NDC_MINIMAX_API_KEY="..."
export NDC_MINIMAX_GROUP_ID="..."
```

### 4.2 配置文件

支持分层配置：

- 项目级：`./.ndc/config.yaml`
- 用户级：`~/.config/ndc/config.yaml`
- 全局级：`/etc/ndc/config.yaml`

优先级：项目 > 用户 > 全局。

## 5. 当前工具体系（默认注册）

- `fs`
- `shell`
- `git`
- `list`
- `read`
- `write`
- `edit`
- `grep`
- `glob`
- `webfetch`
- `websearch`

## 6. 已知限制

- REPL 已实现“事件级”实时渲染（步骤/工具/thinking/time-line 增量刷新），但尚未实现逐 token 文本流式渲染
- gRPC/SSE 时间线订阅已支持实时推送 + 轮询补偿，但尚未接入跨进程持久事件总线
- `crates/storage` 仍在重构中，尚未并入 workspace 主链

## 7. 建议使用流程

1. 先用 `run --message` 做一次性分析任务
2. 再进入 `repl` 做多轮迭代修复
3. 每次改动后要求 Agent 执行 `cargo check` / `cargo test`
4. 最后人工 review 关键改动并提交
