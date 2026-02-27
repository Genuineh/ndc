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

- [x] 权限区独立交互（y/n/a 快捷键）— `baaf076` mpsc+oneshot channel 替换 stdin 阻塞

---

## P0-SEC 深度审计修复

> 来源：2026-02-26 全项目深度审计（52,505 LOC / 665 tests）  
> 原则：安全 → 健壮性 → 架构，每项遵循 Red→Green TDD  
> 深度细化：2026-02-26 逐项代码级调查  
> SEC-Immediate 完成：2026-02-26（6/6 项，+16 新测试，5 次原子提交）

### ✅ P0-SEC-Immediate（立即修复）— 全部完成

#### ✅ SEC-C1 Shell 执行超时失效 — `b6f8858`

- **位置**: `crates/runtime/src/tools/shell.rs`
- **修复**: `_timeout` → `timeout`，`cmd.output()` 用 `tokio::time::timeout()` 包装，新增 `ToolError::Timeout` 变体
- **测试**: +3 新测试（正常完成 / 超时触发 / 超时错误类型）

#### ✅ SEC-C2 路径 `..` 遍历绕过 — `589feb8`

- **位置**: `crates/runtime/src/tools/security.rs`
- **修复**: 新增 `normalize_path()` 逻辑规范化（消除 `..` / `.` 组件），`canonicalize_lossy` 最终回退改用 `normalize_path` 替代原始路径
- **测试**: +2 新测试（normalize_path 单元 + `..` 遍历边界拦截）

#### ✅ SEC-C3 API Key 泄露 + panic — `95a9027`

- **位置**: `crates/core/src/llm/provider/anthropic.rs` + `mod.rs`
- **修复**: `ProviderConfig` 自定义 `Debug`（api_key 仅显示前 4 字符+`***`）；新增 `ProviderError::InvalidConfig`；Anthropic `get_headers()` 改为 `Result`，4 处 `.parse().unwrap()` 替换为 `safe_header_value()?`
- **测试**: +3 新测试（非法 key 返回错误 / 合法 key 成功 / Debug 屏蔽验证）

#### ✅ SEC-H3 权限默认放行 — `dc8e25a`

- **位置**: `crates/interface/src/agent_mode.rs`
- **修复**: 通配符 `"*"` 默认权限 `Allow` → `Ask`；显式 `Allow`: `file_read`, `task_manage`；显式 `Ask`: `shell_execute`, `network`
- **测试**: +3 新测试（通配符默认值 / 只读放行 / 危险操作确认）

#### ✅ SEC-H5 WebFetch SSRF 防护 — `c438f08`

- **位置**: `crates/runtime/src/tools/webfetch.rs`
- **修复**: 新增 `validate_url_safety()`（scheme 白名单 http/https、私有 IP 拦截、blocked hostname）；reqwest 客户端 `redirect(Policy::none())`；新增 `is_private_ip()` 辅助函数
- **测试**: +8 新测试（协议 / 私有 IP / localhost / internal 主机名 / 公网 URL / 无效 URL / loopback / public IP）

#### ✅ SEC-H6 Shell 环境变量控制 — `b6f8858`

- **位置**: `crates/runtime/src/tools/shell.rs`
- **修复**: 新增 `DANGEROUS_ENV_VARS` 黑名单（`LD_PRELOAD` / `LD_LIBRARY_PATH` / `PYTHONPATH` / `NODE_OPTIONS` / `DYLD_INSERT_LIBRARIES`）；白名单补充 `LANG` / `TERM` / `LC_ALL`；`context.env_vars` 也过滤危险变量
- **测试**: +2 新测试（黑名单过滤 / 白名单传递）

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

#### ✅ SEC-C5 MemoryStorage 容量限制 — `bf99bc9`

- **位置**: `crates/storage/src/memory.rs`
- **修复**: HashMap 改为 HashMap + VecDeque 追踪插入顺序；默认 max_tasks/max_memories = 10,000（with_capacity() 可配置）；超容量自动淘汰最早条目；更新已有条目不触发淘汰
- **测试**: +4 新测试（基础 CRUD/task 淘汰/更新不淘汰/memory 淘汰）

#### ✅ SEC-H1 工具输出注入防护 — `161fbc3`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: 新增 sanitize_tool_output()：超过 100K 字符截断 + [truncated] 标记；工具输出用 <tool_output>...</tool_output> XML 标签包裹；messages 和 session_state 均使用 sanitized 内容
- **测试**: +3 新测试（短内容/超限截断/临界值）

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

#### ✅ SEC-H4 文件写入非原子 — `48333c3`

- **位置**: `crates/runtime/src/tools/write_tool.rs` + `edit_tool.rs`
- **修复**: 新增 `atomic_write(path, content)` 辅助函数（write-tmp + rename），WriteTool/EditTool 均改用
- **测试**: +6 新测试（atomic_write helper 基础/覆写、write 原子/覆写/append、edit 原子）

#### ✅ SEC-H7 验证结果 unwrap panic — `6790864`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: match + unwrap 重构为 `if let (true, Some(vr))` 直接解构，消除隐性 panic 路径
- **测试**: 现有 185 测试全绿，逻辑行为不变

#### ✅ SEC-H8 事件广播静默丢弃 — `9c5bde8`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: `let _ = event_tx.send()` → `if let Err(e)` + `tracing::warn!` 记录失败及 receiver 数量
- **测试**: +1 新测试（test_event_broadcast_no_receivers_does_not_panic）

#### ✅ SEC-H9 LSP 子进程超时回收 — `fa7e4bc`

- **位置**: `crates/runtime/src/tools/lsp.rs`
- **修复**: std::process::Command → tokio::process::Command + tokio::time::timeout；新增 run_with_timeout() 辅助函数（默认 60s）；is_available() 改 async（5s 超时）；所有 Command 设 kill_on_drop(true)
- **测试**: +3 新测试（成功/超时/空命令检查）

#### ✅ SEC-H10 Session ID 格式校验 — `563aa19`

- **位置**: `crates/interface/src/grpc.rs`
- **修复**: 新增长度上限 128 + 字符白名单（alphanumeric/-/_）校验，错误消息不再回显原始 ID
- **测试**: +2 新测试（合法 ID 通过 / 注入类 ID 拒绝）

---

### 🟡 P0-SEC-Medium（两周内修复）

#### ✅ SEC-M1 Config 范围校验 — `0c80157` + `e673bc0`

- **位置**: `crates/core/src/config.rs` + `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: YamlLlmConfig::validate()（temperature 0.0..=2.0, max_tokens 1..=1M, timeout 1..=3600）；YamlReplConfig::validate()（max_history 1..=100K, session_timeout 1..=86400）；AgentConfig::validate()（max_tool_calls 1..=200, max_retries 0..=10, timeout_secs 1..=3600）；NdcConfigLoader::load() 加载后自动调用 validate_config()；新增 AgentError::ConfigError 变体
- **测试**: +13 新测试（LLM/REPL/AgentConfig 各项边界）

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

#### ✅ SEC-M8 文件读取大小限制 — `76802a6`

- **位置**: `crates/runtime/src/tools/read_tool.rs`
- **修复**: 读取前 metadata 检查（超过 10MB 拒绝）；/dev/* 和 /proc/* 路径直接拒绝，防止 OOM
- **测试**: +3 新测试（超大文件/dev 路径/proc 路径）

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
