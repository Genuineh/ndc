# NDC TODO / Backlog

> 更新时间：2026-02-27（v11）  
> 已完成里程碑归档：`docs/plan/archive/COMPLETED_MILESTONES.md`  
> 关联文档：`docs/plan/current_plan.md` · `docs/USER_GUIDE.md` · `docs/design/`

## 看板总览

| 优先级 | 状态 | 主题 |
|--------|------|------|
| **P0-D** | ✅ 已完成 | 安全边界与项目级会话隔离 |
| **P0-C** | ✅ 已完成 | Workflow-Native REPL 与实时可观测 |
| **P1-UX** | ✅ 已完成 | REPL TUI 布局与体验重设计（P1-UX-1~6 全部完成） |
| **P0-SEC** | ✅ 已完成 | 深度审计修复（安全 / 健壮性 / 架构） |
| **BugFix** | ✅ 已完成 | Shell 执行修复 + Ctrl+C 任务中断 |
| **P1** | 待开始 | 核心自治能力与治理 |
| **P2** | 待开始 | 多 Agent 与知识回灌体验 |

---

## 活跃工作

当前无活跃 P0 工作项。下一阶段为 P1（核心自治能力与治理）。

### 最近完成

#### Shell 执行命令修复 — `ca066da`

- **位置**: `crates/runtime/src/tools/shell.rs`
- **问题**: LLM 传入完整命令字符串（如 `"echo test"`）时，`Command::new()` 将整个字符串作为可执行文件名查找，报 `No such file or directory`
- **修复**: 当 `args` 为空且 `command` 含空格或 shell 元字符时，自动通过 `sh -c` 执行
- **测试**: +3 新测试（命令字符串 / 管道 / 单独可执行 + args 兼容）

#### Ctrl+C 中断运行任务 — `4ac083c`

- **位置**: `crates/interface/src/repl.rs`, `crates/interface/src/layout_manager.rs`, `crates/core/src/ai_agent/mod.rs`
- **问题**: Ctrl+C 始终退出整个 REPL，无法中断正在运行的 Agent 任务
- **修复**: 处理中按 Ctrl+C 中断当前任务（`JoinHandle::abort()`）并显示 `[Interrupted]`；空闲时 Ctrl+C 退出 REPL
- **新增**: `AgentError::Cancelled` 变体；状态栏动态提示（处理中显示 "Ctrl+C interrupt"）

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

#### ✅ SEC-C4 Session 三锁竞态 — `ae0e1fd`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: 合并 3 个独立 `Arc<Mutex<HashMap>>>` (sessions, project_sessions, project_last_root_session) 为单一 `Arc<Mutex<SessionStore>>` 结构体；`SessionStore::index_session()` 在同一锁作用域内调用；所有 ~10 个 session 方法更新为使用统一 store 锁
- **测试**: +1 并发测试（4 项目 × 10 会话并发写入一致性断言）

#### ✅ SEC-C5 MemoryStorage 容量限制 — `bf99bc9`

- **位置**: `crates/storage/src/memory.rs`
- **修复**: HashMap 改为 HashMap + VecDeque 追踪插入顺序；默认 max_tasks/max_memories = 10,000（with_capacity() 可配置）；超容量自动淘汰最早条目；更新已有条目不触发淘汰
- **测试**: +4 新测试（基础 CRUD/task 淘汰/更新不淘汰/memory 淘汰）

#### ✅ SEC-H1 工具输出注入防护 — `161fbc3`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: 新增 sanitize_tool_output()：超过 100K 字符截断 + [truncated] 标记；工具输出用 <tool_output>...</tool_output> XML 标签包裹；messages 和 session_state 均使用 sanitized 内容
- **测试**: +3 新测试（短内容/超限截断/临界值）

#### ✅ SEC-H2 gRPC 无限并发流 — `fbcd209`

- **位置**: `crates/interface/src/grpc.rs`, `crates/interface/Cargo.toml`
- **修复**: 添加 tower ConcurrencyLimitLayer(64) 限制并发请求；tonic .timeout(300s) 请求级超时；.http2_max_pending_accept_reset_streams(Some(64))；tower 作为可选依赖加入 grpc feature gate
- **常量**: MAX_CONCURRENT_GRPC_REQUESTS=64, GRPC_REQUEST_TIMEOUT_SECS=300

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

#### ✅ SEC-M2 Storage 用 std::sync::Mutex — `e7eaae6`

- **位置**: `crates/storage/src/memory.rs`
- **修复**: `std::sync::Mutex` → `tokio::sync::Mutex`；`.lock().map_err(...)` → `.lock().await`；移除 PoisonError 处理（tokio Mutex 不 poison）
- **测试**: 4 个已有测试全绿

#### ✅ SEC-M3 SQLite 连接池 — `34152f4`

- **位置**: `crates/storage/src/sqlite.rs`
- **修复**: 自定义 `SqliteConnectionManager` 实现 `r2d2::ManageConnection`（connect 打开连接 + WAL pragma，is_valid 执行 `SELECT 1`）；`SqliteStorage` 持有 `Pool<SqliteConnectionManager>`（max_size=4）；`run_sqlite()` 从 pool 获取连接替代每次 `Connection::open()`
- **测试**: +2 新测试（10 并发写入 / 5 次顺序复用），全部 12 测试通过

#### ✅ SEC-M5 消息历史无限增长 — `ae47d55`

- **位置**: `crates/core/src/ai_agent/orchestrator.rs`
- **修复**: 新增 `truncate_messages()` 函数在每次 LLM 调用前裁剪消息历史；保留系统提示(首条) + 最近 MAX_CONVERSATION_MESSAGES(40) 条非系统消息；超出部分替换为占位符
- **测试**: +4 新测试（未达上限/超限/无系统提示/恰好临界）

#### ✅ SEC-M7 生产代码 `.unwrap()` 清理 — `9fd5fa6`

- **位置**: `crates/core/src/todo/mapping_service.rs`, `crates/runtime/src/documentation/mod.rs`, `crates/runtime/src/skill/executor.rs`, `crates/runtime/src/executor.rs`
- **修复**: mapping_service.rs 7 处 RwLock `.unwrap()` → `.expect("todo RwLock poisoned")`；documentation/mod.rs 6 处 RwLock `.unwrap()` → 描述性 `.expect()`，`find('{').unwrap()` → `expect("brace confirmed by contains")`；skill/executor.rs context `.unwrap()` → `.expect("context set above")`；executor.rs `position().unwrap()` → `.expect("step must exist in task")`
- **测试**: 全部 471 core+runtime 测试通过

#### ✅ SEC-M8 文件读取大小限制 — `76802a6`

- **位置**: `crates/runtime/src/tools/read_tool.rs`
- **修复**: 读取前 metadata 检查（超过 10MB 拒绝）；/dev/* 和 /proc/* 路径直接拒绝，防止 OOM
- **测试**: +3 新测试（超大文件/dev 路径/proc 路径）

---

### 🔵 P0-SEC-Structural（持续改进）

#### ✅ SEC-S3 清理旧管线死代码 — `5d3bf2a`

- **位置**: `crates/interface/src/repl.rs`, `crates/interface/src/grpc_client.rs`
- **修复**: repl.rs 删除 ~750 行（TUI_MAX_LOG_LINES 常量、ToolCallCard.round 字段、ChatTurn 结构体、hint() 方法、style_session_log_lines/style_session_log_line/render_inline_markdown/parse_inline_spans/push_log_line/drain_live_execution_events 函数 + 18 个关联死测试）；grpc_client.rs 删除 ~70 行（PooledChannel/delay/is_retryable_error + 关联测试）；SlashCommandSpec.summary 重命名为 _summary
- **注意**: `event_to_lines` 保留（被 `render_execution_events` 生产代码调用）
- **测试**: 全部 242 接口测试通过

#### ✅ SEC-S5 CI 添加 cargo audit — `03f4b14`

- **位置**: `.github/workflows/ci.yml`
- **修复**: 创建 GitHub Actions CI 工作流，包含 4 个 job：cargo fmt --check / cargo clippy -D warnings / cargo test --workspace / rustsec/audit-check；push to main 和 PR 触发
- **修复**: 创建 GitHub Actions CI 工作流，包含 4 个 job：cargo fmt --check / cargo clippy -D warnings / cargo test --workspace / rustsec/audit-check；push to main 和 PR 触发

#### ✅ SEC-S1 拆分三大 God Object（orchestrator.rs + agent_mode.rs + repl.rs 已完成）

- **orchestrator.rs**（~3400 行 → ~2230 行，削减 ~1170 行）✅
  - `session_store.rs` ✅ `a0fc215`：SessionStore + 10 方法 + 10 测试
  - `prompt_builder.rs` ✅ `766fb48`：build_messages + build_working_memory_injector + 6 测试
  - `helpers.rs` ✅ `62b8fce`：6 工具函数 + 2 常量 + 15 测试
  - `conversation_runner.rs` ✅：ConversationRunner 结构体 + run_main_loop + execute_tool_calls + emit_event/workflow_stage/token_usage + 6 测试
- **agent_mode.rs**（~3273 行 → ~1869 行，削减 ~1404 行，43% 缩减）✅
  - `provider_config.rs` ✅ `4142de6`：7 函数（create_provider_config + API key 解析 + model 选择）+ 4 测试
  - `project_index.rs` ✅ `d95ea2e`：ProjectIndexStore + 持久化逻辑 + 4 发现函数 + 1 测试
  - `session_archive.rs` ✅ `ce4ec65`：SessionArchiveStore + 归档逻辑 + 1 测试
  - `permission_engine.rs` ✅ `e04d459`：PermissionRule + ReplToolExecutor + ToolExecutor impl + 11 测试
- **repl.rs**（~7374 行 → ~5224 行，削减 ~2150 行，29% 缩减）✅
  - `chat_renderer.rs` ✅：TuiTheme + ChatEntry + 13 渲染函数（834 行）
  - `input_handler.rs` ✅：InputHistory + ReplTuiKeymap + 补全逻辑 + 24 函数（556 行）
  - `layout_manager.rs` ✅：TuiSessionViewState + DisplayVerbosity + 布局计算 + 30 函数（800 行）
- **修复策略**: 每个子模块作为独立 PR，保持原 pub API 不变（通过 `pub use` re-export）
- **测试总计**: ndc-core 231 通过（+22 新测试）；ndc-interface 212 通过

#### SEC-S2 10 阶段管线缺口评估 ✅ `077dcc8`

- **差距分析文档**: `docs/design/sec-s2-pipeline-gap-analysis.md`
- **完成度**: 4/10 完整 + 4/10 部分 + 2/10 缺失 ≈ 60%
- **完整实现**: Stage 0(Lineage, 5 tests) + Stage 3(Discovery, 17 tests) + Stage 4(WorkingMemory, 7 tests) + Stage 5(Saga, 8 tests)
- **部分实现**: Stage 1(Understand, 结构在未集成) + Stage 2(Decompose, Lint 完整缺 Undo) + Stage 6(Accept, 基础验证) + Stage 8(Document, 内存模型)
- **未实现**: Stage 7(Failure → Invariant) + Stage 9(Complete/Telemetry)
- **关键缺口**: orchestrator 未接入已实现模块；失败学习闭环缺失
- **建议**: 渐进补齐（P0 打通 orchestrator 调用链 → P1 实现 Stage 7 → P2 实现 Stage 9）

#### SEC-S4 补充关键路径测试 ✅ `5e5bc04`

- **当前覆盖**: core(209) / runtime(270) / interface(249) / storage(18) / decision(21) ≈ 767 总测试
- **新增 18 个测试**:
  - MemoryStorage: +6 (CRUD, 并发写入, 零容量边界, list_tasks, get_nonexistent)
  - 并发 Session: +2 (10 并行 get_or_create, 5 并行 save + latest 追踪)
  - 权限回退: +4 (通配符 fallback, 无通配符默认 Ask, 未知工具分类, git 操作细分)
  - 文件工具边界: +6 (空文件/单行/二进制/不存在 for ReadTool; LineTrimmed 回退/空内容删除 for EditTool)

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
| P0-SEC | 2026-02 | 深度安全审计（52K LOC，修复 20+ 项，+80 新测试） |
| P1-UX | 2026-02 | TUI 布局重设计（6 区动态布局 / ChatEntry / TuiTheme / 三级 Verbosity） |
| BugFix | 2026-02 | Shell 命令执行修复 + Ctrl+C 任务中断 |
| 工程治理 | 2026-02 | 清理空 crate、storage 独立、edition 2024 统一、God Object 拆分 |

> 详细实现记录见 `docs/plan/archive/COMPLETED_MILESTONES.md`

---

## 验收门禁（合并前）

1. `cargo check` 通过
2. `cargo test -q` 通过
3. 对应主链 smoke 测试通过
4. 文档同步更新
