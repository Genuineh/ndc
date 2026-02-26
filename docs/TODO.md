# NDC TODO / Backlog

> 更新时间：2026-02-26（v10）  
> 已完成里程碑归档：`docs/plan/archive/COMPLETED_MILESTONES.md`  
> 关联文档：`docs/plan/current_plan.md` · `docs/USER_GUIDE.md` · `docs/design/`

## 看板总览

| 优先级 | 状态 | 主题 |
|--------|------|------|
| **P0-D** | ✅ 已完成 | 安全边界与项目级会话隔离 |
| **P0-C** | ✅ 已完成 | Workflow-Native REPL 与实时可观测 |
| **P1-UX** | ✅ 已完成 | REPL TUI 布局与体验重设计（P1-UX-1~6 全部完成） |
| **P0-SEC** | 🔴 紧急 | 深度审计修复（安全 / 健壮性 / 架构） |
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

## P0-SEC 深度审计修复

> 来源：2026-02-26 全项目深度审计（52,505 LOC / 665 tests）  
> 原则：安全 → 健壮性 → 架构，每项遵循 Red→Green TDD  
> 深度细化：2026-02-26 逐项代码级调查

### 🔴 P0-SEC-Immediate（立即修复）

#### SEC-C1 Shell 执行超时失效

- **位置**: `crates/runtime/src/tools/shell.rs` L78-102
- **现状**: `execute()` 提取 `_timeout` 参数（L78-81 带 `_` 前缀，明确未使用），`Command::output().await`（L99-102）无任何超时包装，恶意/死循环命令可无限挂起
- **修复步骤**:
  1. Red: 测试 shell 执行超时 → 超过阈值返回 `ToolError::Timeout`
  2. Green: 去掉 `_` 前缀，用 `tokio::time::timeout(Duration::from_secs(timeout), cmd.output().await)` 包装
  3. 补充测试：正常命令在超时内完成 / `sleep 999` 触发超时错误
- **影响范围**: `ShellTool::execute()` 单一入口，不影响其他工具
- **现有测试**: ❌ 无超时相关测试

#### SEC-C2 路径边界绕过（symlink）

- **位置**: `crates/runtime/src/tools/security.rs` L139-180
- **现状**:
  - `canonicalize_lossy()`（L139-154）：若文件不存在则对 parent 做 `std::fs::canonicalize` + join filename，但 **不检查 symlink 目标**
  - `enforce_path_boundary()`（L168-180）：用 `resolved.starts_with(&project_root)` 判断边界
  - 攻击路径：在项目内创建 symlink → 指向项目外目录 → `canonicalize` 解析为项目内路径 → `starts_with` 通过 → 实际访问外部文件
- **修复步骤**:
  1. Red: 测试 symlink 指向项目外 → `enforce_path_boundary` 应拒绝
  2. Green: canonicalize 后检查 `fs::symlink_metadata(&resolved)` 是否为 symlink，若是则对 `fs::read_link` 结果再验证边界
  3. 增加 `is_symlink_escaping_boundary()` 辅助函数
- **影响范围**: 所有文件工具（read/write/edit/delete）经由 `enforce_path_boundary` 调用
- **现有测试**: ⚠️ 有边界检查测试（L360-450），但无 symlink 场景

#### SEC-C3 API Key 泄露 + panic

- **位置**: 4 个 Provider 实现
  - `crates/core/src/llm/provider/anthropic.rs` L56-68：`get_headers()` 中 **4 处** `.parse().unwrap()`（api_key×2, version×1, org×1）
  - `crates/core/src/llm/provider/openai.rs` L57-59：`get_auth_header()` 返回 String，后续 header 设置处需检查
  - `crates/core/src/llm/provider/minimax.rs` L76-78：`format!("Bearer {}", api_key)` 同模式
  - `crates/core/src/llm/provider/mod.rs` L202：`ProviderConfig` derive `Debug` 暴露 `api_key` 字段
- **现状**: API Key 含非 ASCII 或控制字符时（如从环境变量误读 `\n`），`.parse::<HeaderValue>().unwrap()` 直接 panic，且 panic 消息包含完整 key
- **修复步骤**:
  1. Red: 测试包含 `\n` 的 API Key → 不 panic，返回 `LlmError::InvalidConfig`
  2. Green:
     - 所有 `.parse().unwrap()` → `.parse().map_err(|_| LlmError::InvalidApiKey("invalid header chars"))?`
     - `ProviderConfig` 手写 `impl Debug`，`api_key` 字段输出为 `"sk-***"`
  3. 新增统一 `fn safe_header_value(s: &str) -> Result<HeaderValue, LlmError>` 辅助函数
- **影响范围**: Anthropic / OpenAI / MiniMax / OpenRouter 四个 provider
- **现有测试**: ⚠️ 有 provider 测试但不覆盖非法字符场景

#### SEC-H3 权限默认放行

- **位置**: `crates/interface/src/agent_mode.rs` L1715-1790
- **现状**:
  - `AgentModeConfig::default()`（L1715-1720）设置 `"*" → PermissionRule::Allow`
  - `resolve_permission_rule()`（L1784-1790）：未知 key → 匹配 `"*"` → `Allow`
  - 只有 `file_write` / `git_commit` / `file_delete` 显式设为 `Ask`
  - 新增工具（如 `lsp_invoke` / `network_custom`）自动获得 `Allow` 权限，无任何确认
- **修复步骤**:
  1. Red: 测试未知操作 `"unknown_tool"` → `resolve_permission_rule` 返回 `Ask`（而非 `Allow`）
  2. Green: `"*"` 默认值从 `PermissionRule::Allow` 改为 `PermissionRule::Ask`
  3. 显式添加安全只读操作（`file_read` / `glob` / `grep`）为 `Allow`
- **影响范围**: 所有工具执行前的权限检查路径（L1987-2004）
- **现有测试**: ❌ 无未知操作回退测试

#### SEC-H5 Web 工具 SSRF 风险

- **位置**: `crates/runtime/src/tools/websearch.rs` L32-57
- **现状**:
  - URL 使用 DuckDuckGo API 硬编码（`https://api.duckduckgo.com/?q=...`），query 经 `urlencoding::encode()` 编码
  - 当前 SSRF 风险较低（URL 固定），但 reqwest 默认跟随重定向，无 `redirect(Policy::none())`
  - 未来如支持用户自定义搜索 URL 则完全暴露
- **修复步骤**:
  1. Red: 测试 reqwest client 不跟随重定向到内网地址
  2. Green:
     - `reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())` 禁用重定向
     - 添加 `validate_url_safety(url)` 检查 scheme(`https` only) + resolve IP 非私有段
  3. 若未来开放自定义 URL，此校验函数即时生效
- **影响范围**: `WebSearchTool::search()` 单一入口
- **现有测试**: ❌ 无 URL 安全测试

#### SEC-H6 Shell 环境变量控制

- **位置**: `crates/runtime/src/tools/shell.rs` L90-97
- **现状**:
  - 白名单仅 4 项：`PATH` / `HOME` / `USER` / `SHELL`（L91-92）
  - 但 `self.context.env_vars`（L94）内容来源不受控 — 若 config/用户输入注入 `LD_PRELOAD` / `PYTHONPATH` 等，子进程可被劫持
  - 白名单本身缺少 `LANG` / `LC_ALL`（影响命令输出编码）
- **修复步骤**:
  1. Red: 测试 `context.env_vars` 含 `LD_PRELOAD` → 被过滤，不传递给子进程
  2. Green:
     - 新增环境变量黑名单常量：`DANGEROUS_ENV_VARS = ["LD_PRELOAD", "LD_LIBRARY_PATH", "PYTHONPATH", "NODE_OPTIONS", "DYLD_INSERT_LIBRARIES"]`
     - 在 L94 条件中增加 `!DANGEROUS_ENV_VARS.contains(&key.as_str())`
     - 白名单补充 `LANG` / `TERM` / `LC_ALL`
  3. 补充测试：黑名单变量被过滤 / 白名单变量正常传递
- **影响范围**: `ShellTool::execute()` 环境变量设置段
- **现有测试**: ❌ 无环境变量过滤测试

---

### 🟠 P0-SEC-Short（一周内修复）

#### SEC-C4 Session 三锁竞态

- **位置**: `crates/core/src/ai_agent/orchestrator.rs` L203-226, L524-530, L609-620
- **现状**:
  - `AgentOrchestrator` 持有 3 个独立 `Arc<Mutex<HashMap>>>`：`sessions`(L214), `project_sessions`(L217), `project_last_root_session`(L220)
  - `save_session()`(L524)：先锁 `sessions` 写入，释放后调 `index_session()`
  - `index_session()`(L609)：依次锁 `project_sessions`(L610) 和 `project_last_root_session`(L618)
  - **竞态窗口**：线程 A 释放 `sessions` 锁后、获取 `project_sessions` 锁前，线程 B 可修改 `sessions`，导致索引指向已过期/不存在的 session
- **修复步骤**:
  1. Red: 并发测试 — 两个线程同时 `save_session` 不同 project → 索引一致性断言
  2. Green: 合并三个 HashMap 为单一 `SessionStore` 结构，用单一 `Arc<Mutex<SessionStore>>` 保护
     ```rust
     struct SessionStore {
         sessions: HashMap<String, AgentSession>,
         project_sessions: HashMap<String, Vec<String>>,
         project_last_root: HashMap<String, String>,
     }
     ```
  3. 重构 `save_session` / `index_session` 在同一锁内完成所有操作
- **影响范围**: `AgentOrchestrator` 所有 session 相关方法（~10 个方法）
- **现有测试**: ⚠️ 有 session 测试但无并发场景

#### SEC-C5 MemoryStorage 无容量限制

- **位置**: `crates/storage/src/memory.rs` L14-46（全文件）
- **现状**:
  - `tasks: Mutex<HashMap<TaskId, Task>>`(L15) 和 `memories: Mutex<HashMap<MemoryId, MemoryEntry>>`(L16) 无上限
  - `save_task()`(L26-30) / `save_memory()`(L42-46) 直接 `insert`，无淘汰策略
  - `list_tasks()`(L33-36) 返回全量 `.values().cloned().collect()`
- **修复步骤**:
  1. Red: 测试插入超过容量上限 → 最早条目被淘汰
  2. Green:
     - 添加 `max_tasks: usize` / `max_memories: usize` 配置（默认 10,000）
     - 替换 `HashMap` 为 `lru::LruCache`（或自实现 FIFO 淘汰）
     - `insert` 前检查容量，超限自动移除最旧条目
  3. 补充 list 操作分页支持（`limit` / `offset` 参数）
- **影响范围**: `MemoryStorage` 实现，`Storage` trait 接口可能需新增分页参数
- **现有测试**: ❌ 无（memory.rs 0 测试）

#### SEC-H1 工具输出注入 prompt

- **位置**: `crates/core/src/ai_agent/orchestrator.rs` L935-950
- **现状**:
  - 工具执行结果 `result.content` 被 **3 次无过滤复制**（L938 message push, L943 session_state, L947 tool_results）
  - 无截断、无边界标记、无特殊字符转义
  - 攻击者可通过工具输出注入 LLM 指令（prompt injection），或输出超大内容耗尽 token
- **修复步骤**:
  1. Red: 测试工具输出超过 `MAX_TOOL_OUTPUT_CHARS` → 被截断 + 附加 `[truncated]` 标记
  2. Green:
     - 新增常量 `MAX_TOOL_OUTPUT_CHARS = 100_000`
     - `result.content` 在推入 messages 前截断
     - 工具输出用 `<tool_output>...</tool_output>` XML 标签包裹，作为 LLM 边界标记
  3. 考虑敏感内容检测（如 `-----BEGIN RSA PRIVATE KEY-----`）
- **影响范围**: `run_main_loop` 中的工具结果处理段
- **现有测试**: ❌ 无工具输出边界测试

#### SEC-H2 gRPC 无限并发流

- **位置**: `crates/interface/src/grpc.rs` L1091-1118
- **现状**:
  - `tonic::transport::Server::builder()` 直接 `.serve()`，未加任何 tower 中间件(L1111-1118)
  - 流式端点 `subscribe_session_timeline`(L324-367) 每连接 `tokio::spawn` 新任务 + `mpsc::channel(100)`，无并发上限
  - 攻击者可创建无限流连接耗尽内存和文件描述符
- **修复步骤**:
  1. Red: 测试超过 `MAX_CONCURRENT_STREAMS` 连接 → 拒绝新连接
  2. Green:
     - 引入 `tower::ServiceBuilder` 中间件栈
     - 添加 `tower::limit::ConcurrencyLimitLayer::new(64)` 限制并发
     - 添加 `tower::timeout::TimeoutLayer` 限制流存活时间
     - tonic server 设置 `.http2_max_pending_accept_reset_streams(Some(64))`
  3. 考虑按 IP 限流（需 tonic 扩展或 tower 中间件）
- **影响范围**: gRPC server 启动代码，所有流式端点间接受益
- **现有测试**: ❌ 无并发/压力测试

#### SEC-H4 文件写入非原子

- **位置**:
  - `crates/runtime/src/tools/write_tool.rs` L66-89：`fs::write(&path, content)` 直接覆写
  - `crates/runtime/src/tools/edit_tool.rs` L295：`fs::write(&path, &result.0)` 直接覆写
- **现状**:
  - 写入中断（断电/panic）→ 文件损坏，内容丢失无备份
  - append 模式先 `read_to_string` 再 `write`：TOCTOU（读写间文件可被其他进程修改）
- **修复步骤**:
  1. Red: 测试写入后文件内容正确 / 模拟写入中断（temp 文件存在但未 rename）
  2. Green: 新增 `atomic_write(path, content)` 辅助函数：
     ```rust
     async fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
         let tmp = path.with_extension("tmp");
         fs::write(&tmp, content).await?;
         fs::rename(&tmp, path).await?;
         Ok(())
     }
     ```
  3. write_tool / edit_tool 共用此函数
- **影响范围**: `WriteTool::execute()` + `EditTool::execute()`
- **现有测试**: ⚠️ 有基础写入测试，无原子性/中断场景

#### SEC-H7 验证结果 unwrap panic

- **位置**: `crates/core/src/ai_agent/orchestrator.rs` L1038, L1050
- **现状**:
  - L1038: `verification_result.as_ref().unwrap()` → 调 `generate_continuation_prompt`
  - L1050: `verification_result.as_ref().unwrap()` → 调 `generate_feedback_message`
  - 当 `should_verify = false` 时 `verification_result = None`，但 `needs_continuation` 在 `_ => false` 分支匹配 None → 不触发 unwrap
  - **隐性风险**: 逻辑分支变化（如新增 VerificationResult 变体）可能导致 match 未覆盖 → panic
- **修复步骤**:
  1. Red: 测试 `verification_result = None` 时不 panic
  2. Green: 将 `unwrap()` 替换为 `if let Some(vr) = verification_result.as_ref()` guard
  3. 或重构为 match arm 内直接解构 `Some(vr)`
- **影响范围**: 验证续跑逻辑，2 处 unwrap
- **现有测试**: ⚠️ 有验证测试但未覆盖 None 路径

#### SEC-H8 事件广播静默丢弃

- **位置**: `crates/core/src/ai_agent/orchestrator.rs` L236
- **现状**:
  - `emit_event()`(L230-240) 中 `let _ = self.event_tx.send(...)` 静默丢弃发送错误
  - broadcast channel 容量 2048（L325），缓冲区满时新事件丢失
  - UI 侧（REPL/gRPC）接收不到事件 → 显示过期/不完整的 session timeline
- **修复步骤**:
  1. Red: 测试 channel 满时 emit_event 记录 warn 日志
  2. Green:
     ```rust
     if let Err(e) = self.event_tx.send(event) {
         warn!("Event broadcast failed ({}  receivers): {}", self.event_tx.receiver_count(), e);
     }
     ```
  3. 可选：添加 metrics counter 统计丢弃事件数
- **影响范围**: `emit_event` 单一函数
- **现有测试**: ❌ 无 channel 溢出测试

#### SEC-H9 LSP 子进程无超时回收

- **位置**: `crates/runtime/src/tools/lsp.rs` 4 处 `Command::output()`
  - L80-82: `is_available()` — `cmd.status()` 同步阻塞
  - L139-141: `run_rust_analyzer_diagnostics()` — `cargo check` 无超时
  - L233-238: `run_eslint_diagnostics()` — `npx eslint` 无超时
  - L319-325: `run_pyright_diagnostics()` — `npx pyright` 无超时
- **现状**: 所有 `Command::output()` 无 timeout wrapper，`cargo check` 在大项目或依赖下载时可挂起数分钟
- **修复步骤**:
  1. Red: 测试 LSP 诊断超时 → 返回 `Err("diagnostic timeout")`
  2. Green: 统一封装 `run_with_timeout(cmd, timeout_secs)`:
     ```rust
     async fn run_with_timeout(cmd: &mut Command, secs: u64) -> Result<Output, String> {
         tokio::time::timeout(Duration::from_secs(secs), cmd.output())
             .await
             .map_err(|_| "LSP diagnostic timeout".to_string())?
             .map_err(|e| e.to_string())
     }
     ```
  3. 默认超时 60s，可配置
  4. 超时后 kill 子进程（通过 `Command::kill_on_drop(true)` 或显式 kill）
- **影响范围**: `LspTool` 所有诊断方法
- **现有测试**: ❌ 无超时测试

#### SEC-H10 Session ID 无格式校验

- **位置**: `crates/interface/src/grpc.rs` L153-171
- **现状**:
  - `validate_requested_session()`(L153) 仅检查 `is_empty()`，不验证格式
  - Session ID 由 `ulid::Ulid::new().to_string()` 生成（26 字符 alphanumeric）
  - 恶意输入超长字符串 / 含换行符 / ANSI escape → 日志注入、错误消息污染
  - 错误返回中直接拼接 `requested_session_id`（L163-165），可注入
- **修复步骤**:
  1. Red: 测试非法 session_id（超长、含换行、非 alphanumeric）→ 返回 `InvalidArgument`
  2. Green: 在 `validate_requested_session` 开头添加：
     ```rust
     if !requested_session_id.is_empty() {
         if requested_session_id.len() > 128
             || !requested_session_id.chars().all(|c| c.is_ascii_alphanumeric())
         {
             return Err(tonic::Status::invalid_argument("invalid session ID format"));
         }
     }
     ```
  3. 错误消息中不回显原始 session_id
- **影响范围**: 所有 gRPC 端点接收 session_id 的入口
- **现有测试**: ❌ 无格式校验测试

---

### 🟡 P0-SEC-Medium（两周内修复）

#### SEC-M1 Config 无范围校验

- **位置**:
  - `crates/core/src/config.rs` L104-140：`YamlLlmConfig`（temperature/max_tokens/timeout 无边界）
  - `crates/core/src/ai_agent/orchestrator.rs` L70-89：`AgentConfig`（max_tool_calls/max_retries/timeout_secs 无边界）
  - `crates/core/src/config.rs` L130-140：`YamlReplConfig`（max_history/session_timeout 无边界）
- **现状**: 所有数值字段由 serde 直接反序列化，无校验。`temperature: -100.0` 或 `max_tokens: u32::MAX` 均可通过
- **修复步骤**:
  1. Red: 测试不合法 config 值 → 返回 `ConfigError::ValidationFailed`
  2. Green: 为每个 config struct 添加 `fn validate(&self) -> Result<(), ConfigError>`：
     - `temperature`: 0.0..=2.0
     - `max_tokens`: 1..=1_000_000
     - `timeout`: 1..=3600
     - `max_history`: 1..=100_000
     - `max_tool_calls`: 1..=200
     - `max_retries`: 0..=10
  3. 在 config 加载后统一调用 `validate()`
- **现有测试**: ❌ 无校验测试

#### SEC-M2 Storage 用 std::sync::Mutex

- **位置**: `crates/storage/src/memory.rs` L7, L15-16
- **现状**: `use std::sync::Mutex`，在 `#[async_trait] impl Storage` 中使用
- **风险**: 虽未跨 `.await` 持锁，但 std Mutex 阻塞 tokio worker thread，高争用时降低吞吐
- **修复步骤**:
  1. Red: 现有测试仍通过（纯重构不改行为）
  2. Green: `std::sync::Mutex` → `tokio::sync::Mutex`，`.lock().map_err(...)` → `.lock().await`
  3. 移除 `map_err(|e| e.to_string())` 对 PoisonError 的处理（tokio Mutex 不会 poison）
- **影响范围**: `MemoryStorage` 全部方法
- **现有测试**: ❌ 无（结合 SEC-C5 一起补）

#### SEC-M3 SQLite 无连接池

- **位置**: `crates/storage/src/sqlite.rs` L120-138
- **现状**: `run_sqlite()` 辅助函数每次调用做 `rusqlite::Connection::open(&path)` 新建连接，通过 `spawn_blocking` 执行
- **修复步骤**:
  1. Red: 测试并发 10 次 save_task → 全部成功（当前也应通过，做 baseline）
  2. Green:
     - 引入 `r2d2_sqlite` 连接池，`SqliteStorage` 持有 `r2d2::Pool<SqliteConnectionManager>`
     - `run_sqlite` 改为从 pool 获取连接：`pool.get().map_err(...)?`
     - 配置池大小 `max_size = 4`（SQLite WAL 模式支持并发读）
  3. Cargo.toml 添加 `r2d2 = "0.8"`, `r2d2_sqlite = "0.24"`（feature-gated）
- **影响范围**: `SqliteStorage` 初始化 + `run_sqlite` 辅助函数
- **现有测试**: ⚠️ 8 个基础 CRUD 测试

#### SEC-M5 消息历史无限增长

- **位置**: `crates/core/src/ai_agent/orchestrator.rs` L639-1100（`run_main_loop`）
- **现状**:
  - `messages: Vec<Message>` 在循环中仅增不减：每轮 +1 assistant + N tool_results + 可能 verification
  - `max_tool_calls: 50` 默认值下，单 session 可积累 200-500 条消息，每条可数 KB
  - 无滑动窗口、无摘要压缩、无 token 计数上限检查
- **修复步骤**:
  1. Red: 测试消息超过阈值后 → 旧消息被压缩/移除（保留 system prompt + 最近 N 轮）
  2. Green: 在 LLM 调用前添加 `truncate_messages(&mut messages, max_context_tokens)`:
     - 保留 system prompt（首条）
     - 保留最近 `N` 轮对话（默认 20 轮）
     - 中间区域替换为 `[earlier conversation summarized]` 占位
  3. 可选进阶：调用 LLM 做摘要压缩（需评估成本）
- **影响范围**: `run_main_loop` 中 LLM 调用前的消息列表
- **现有测试**: ❌ 无消息管理测试

#### SEC-M7 生产代码 `.unwrap()` 清理

- **位置**: 全项目 659 处，重点清理：
  - `orchestrator.rs` L1038/1050：verification_result（已在 SEC-H7 覆盖）
  - `todo/mapping_service.rs` L313/431/470：`RwLock.read().unwrap()` / `.write().unwrap()` — 锁中毒后级联 panic
  - `anthropic.rs` L60-65：已在 SEC-C3 覆盖
  - `shell.rs` L81：`.unwrap_or(self.context.timeout_seconds)` — 安全（有默认值）
- **修复步骤**:
  1. 按 crate 分批次清理，优先级：core > runtime > interface > storage
  2. `RwLock.unwrap()` → `.map_err(|_| XxxError::LockPoisoned)?` 或 `expect("reason")`
  3. 每批次对应一个原子提交
- **影响范围**: 逐步覆盖，不一次性改动
- **现有测试**: 各 crate 现有测试确保重构不破坏行为

#### SEC-M8 文件读取无大小限制

- **位置**: `crates/runtime/src/tools/read_tool.rs` L37-75
- **现状**: `fs::read_to_string(&path)`(L64) 无大小检查，读完后才计算 `total_bytes`(L66)
- **攻击**: 指定 `/dev/zero` 或 50GB 文件 → OOM
- **修复步骤**:
  1. Red: 测试超过大小限制的文件 → 返回 `ToolError::FileTooLarge`
  2. Green: 在读取前添加 metadata 检查：
     ```rust
     let meta = fs::metadata(&path).await.map_err(ToolError::Io)?;
     const MAX_READ_SIZE: u64 = 10 * 1024 * 1024; // 10MB
     if meta.len() > MAX_READ_SIZE {
         return Err(ToolError::InvalidArgument(format!(
             "File too large: {} bytes (max {})", meta.len(), MAX_READ_SIZE
         )));
     }
     ```
  3. 对特殊文件 `/dev/*` / `/proc/*` 直接拒绝
- **影响范围**: `ReadTool::execute()` 入口
- **现有测试**: ❌ 无文件大小测试

---

### 🔵 P0-SEC-Structural（持续改进）

#### SEC-S3 清理旧管线死代码

- **位置**: `crates/interface/src/repl.rs`
  - `push_log_line()`(L3632)：仅被死代码链调用
  - `drain_live_execution_events()`(L3646)：无活跃调用方
  - `event_to_lines()`(L3700)：仅被 drain 和测试调用
  - `style_session_log_lines()`(L2282)：仅被测试调用
- **修复步骤**:
  1. 删除 4 个函数及其关联测试（`test_push_log_line_capped` 等）
  2. `cargo check` 确认 12 条 dead_code 警告消除
  3. 若 `event_to_lines` 仍在 `render_execution_events`(L4446) 使用，则保留并仅删除 drain/push
- **预估**: 删除 ~400 行 + 相关测试 ~100 行

#### SEC-S5 CI 添加 cargo audit

- **现状**: 项目无 `.github/workflows/` 目录，无 CI 配置文件
- **修复步骤**:
  1. 创建 `.github/workflows/ci.yml`
  2. 包含：`cargo check` / `cargo test` / `cargo clippy` / `cargo audit` / `cargo fmt --check`
  3. 可选：`cargo deny check` 做更全面的许可证 + 漏洞扫描

#### SEC-S1 拆分三大 God Object

- **orchestrator.rs**（~3400 行，31+ 方法）→ 提取：
  - `session_store.rs`：SessionStore + get_or_create/save/hydrate/index 等 ~10 方法
  - `conversation_runner.rs`：run_main_loop + build_messages + execute_tool_calls
  - `prompt_builder.rs`：build_system_prompt + build_messages 模板逻辑
- **agent_mode.rs**（~2800 行，65+ 方法）→ 提取：
  - `provider_config.rs`：create_provider_config + API key 解析 + model 选择
  - `project_index.rs`：ProjectIndexStore + 持久化逻辑
  - `session_archive.rs`：SessionArchiveStore + 归档逻辑
  - `permission_engine.rs`：resolve_permission_rule + classify_permission
- **repl.rs**（~5600 行，100+ 方法）→ 提取：
  - `chat_renderer.rs`：style_chat_entries + render_inline_markdown + 主题渲染
  - `input_handler.rs`：输入解析 + 历史 + 多行编辑
  - `layout_manager.rs`：5-6 区布局计算 + 响应式调整
- **修复策略**: 每个子模块作为独立 PR，保持原 pub API 不变（通过 `pub use` re-export）

#### SEC-S2 10 阶段管线缺口评估

- **设计**（`docs/ENGINEERING_CONSTRAINTS.md`）：10 阶段 Lineage → Understand → Decompose → Discovery → WorkingMemory → Develop → Accept → Failure → Document → Complete
- **已实现**: Stage 1(Understand 部分) + Stage 5(Develop) + Stage 6(Accept 基础验证)
- **部分实现**: Stage 3(Discovery — `crates/runtime/src/discovery/` 存在但未集成) + Stage 4(WorkingMemory — 有 `working_memory.rs` 但注入有限)
- **未实现**: Stage 0(Lineage) + Stage 2(Decompose) + Stage 7(Failure) + Stage 8(Document) + Stage 9(Complete)
- **行动**: 撰写差距分析文档，决定是补齐实现还是收敛设计文档

#### SEC-S4 补充关键路径测试

- **当前覆盖**: core(142) / runtime(58) / interface(23) / storage(8) / decision(10) = 241 总测试
- **缺口**:
  - storage: **0 测试** for MemoryStorage（仅 SQLite 有 8 个）
  - 无跨项目隔离 e2e（多 project 互不干扰）
  - 无并发 session 竞态测试
  - 无 gRPC 流清理/断线重连测试
  - 无 storage 故障恢复测试
- **优先补充**: MemoryStorage 基础 CRUD (4) + 并发 session (2) + 权限回退 (2) + 文件工具边界 (4)

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
