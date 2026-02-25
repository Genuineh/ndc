# P0-D Design: Security Boundary and Project-Scoped Session Isolation

> 状态：Draft  
> 更新时间：2026-02-25  
> 关联：`docs/TODO.md`、`docs/plan/current_plan.md`、`opencode/specs/project.md`

## 1. Context

P0-D 的目标是把 NDC 当前“可用但松散”的安全与会话机制，收敛为可验证的工程基线：

1. 危险操作必须有统一、可审计、可测试的拦截路径。
2. 会话必须按项目隔离，`resume/continue` 只能回到当前项目上下文。
3. REPL 必须实时展示权限状态，用户能明确看到“为何阻塞/拒绝”。

## 2. Scope and Non-Goals

### In Scope

1. 项目标识（`project_id`）计算与持久化。
2. Session 元数据扩展（`project_id/worktree/directory`）。
3. 项目级 `resume/continue` 语义与跨项目保护。
4. 权限网关统一接入（非仅 REPL 层）。
5. `external_directory` 权限语义与判定。
6. 权限事件实时可观测（timeline/status line）。

### Non-Goals (P0-D 不做)

1. 容器级沙箱隔离（Docker/VM）实现。
2. 多租户远程权限模型（server ACL）。
3. 全量 RBAC/组织级授权平台。

## 3. Architecture

### 3.1 Project Identity Model

`project_id` 规则：

1. Git 项目：`git_root_commit_sha`（与 OpenCode 思路一致，保证 worktree 一致归属）。
2. 非 Git 项目：`sha256(canonical_project_root_path)`。
3. 任何模式都要记录：
   - `project_id`
   - `project_root`
   - `working_dir`
   - `worktree`（Git 时可与 root 区分）

实现建议（Rust）：

1. 新增 `ProjectIdentityResolver`（runtime/interface 可复用）。
2. `AgentModeManager::enable` 注入项目身份，不再只生成随机会话前缀。
3. 在状态命令与 REPL 状态栏输出 `project_id`（截断显示）。

### 3.2 Session Metadata and Index

`AgentSession` 新增字段：

1. `project_id: String`
2. `project_root: PathBuf`
3. `working_dir: PathBuf`
4. `is_root_session: bool`

`SessionManager` 新增索引：

1. `project_sessions: HashMap<String, Vec<String>>`
2. `project_last_root_session: HashMap<String, String>`

强约束：

1. 写入 session 时必须同步更新两类索引。
2. 删除/过期清理时必须反向清理索引，防止脏指针。

### 3.3 Resume Semantics

统一语义：

1. `--continue` / `/resume`：
   - 只在当前 `project_id` 下寻找最近 root session。
2. `--session <id>`：
   - 默认要求 `session.project_id == current_project_id`。
   - 不一致时拒绝，并提示 `--allow-cross-project-session`（显式越权开关）。
3. `--fork`：
   - 只允许在同项目 session 上 fork（跨项目默认拒绝）。

错误文案要求（必须可定位）：

1. 包含请求 session id。
2. 包含当前/目标 project id（短 ID）。
3. 给出下一步命令建议。

### 3.4 Permission Gateway

原则：权限判定要在“工具执行主入口”生效，不允许只在 REPL UI 层判定。

执行链：

1. `ToolExecutor.execute_tool(...)`
2. `PermissionGateway.check(request)`
3. `allow -> execute` / `ask -> pending` / `deny -> fail`
4. 结果写入 execution event（用于 REPL/gRPC/SSE）

策略（默认）：

1. `Critical`：直接拒绝（不可确认放行）。
2. `High`：必须确认。
3. `Medium`：默认确认（可配置降级）。
4. `Low/Safe`：自动放行。

### 3.5 External Directory Boundary

新增权限语义：`external_directory`

判定规则：

1. 目标路径不在 `project_root` 内，且不在 `worktree` 安全范围内。
2. shell 命令解析出路径参数后，逐个做规范化判定。
3. fs/read/write/edit/list/glob/grep 都必须共享同一判定函数。

行为：

1. 默认 `ask`。
2. 用户 `always allow` 后记录 pattern 级规则（可过期）。
3. 规则命中日志必须落 timeline。

### 3.6 Real-time Observability

REPL 新增实时状态输出：

1. `permission_state`: `idle | waiting | allowed | denied`
2. `permission_type`: `shell_execute | file_write | external_directory | ...`
3. `risk_level`: `low|medium|high|critical`
4. `blocked_reason`: 结构化说明（可脱敏）

事件模型（建议）：

1. `PermissionAsked`
2. `PermissionApproved`
3. `PermissionRejected`
4. `PermissionBypassedByRule`

要求：`/timeline` 与 gRPC/SSE 字段一致。

### 3.7 Config and Backward Compatibility

新增配置项（建议）：

1. `security.permission.enforce_gateway = true`
2. `security.external_directory.default_action = "ask"`
3. `session.cross_project.allow = false`
4. `session.resume.scope = "current_project"`

兼容原则：

1. 老配置缺失时使用安全默认值。
2. 旧客户端忽略新增事件字段不崩溃。
3. CLI 旧参数行为保持，但在跨项目场景增加明确拒绝。

## 4. Implementation Plan (P0-D1 ~ P0-D6)

### P0-D1: Project Identity

交付：

1. `ProjectIdentityResolver` + 单测。
2. `AgentSession` 元数据扩展。
3. REPL `/status` 输出项目信息。

退出条件：

1. 同项目不同子目录 `project_id` 一致。
2. 不同项目 `project_id` 不一致。

### P0-D2: Project-Scoped Session and Resume

交付：

1. `SessionManager` 项目索引。
2. `--continue` 与 `/resume` 按项目恢复。
3. `--session` 跨项目拒绝与 override。

退出条件：

1. A/B 项目并行运行不串线。
2. 错误文案包含 project 对比信息。

### P0-D3: Unified Permission Gateway

交付：

1. 工具执行入口前置网关（非 UI）。
2. 风险分级执行策略落地。
3. `external_directory` 类型接入。

退出条件：

1. 高危/越界路径在非 REPL 通道也会拦截。
2. 拦截结果可回放（timeline 可见）。

### P0-D4: Real-Time Permission Visualization

交付：

1. REPL session 面板展示权限事件详情。
2. 状态栏增加阻塞原因与风险级别。

退出条件：

1. 用户能看到“具体请求 -> 用户动作 -> 执行结果”闭环。

### P0-D5: Tests and Regression Suite

交付：

1. 单测（project id、path boundary、risk mapping、resume guard）。
2. 集成测试（A/B 隔离、continue 归属、cross-project deny）。
3. E2E（REPL 可观测字段断言）。

退出条件：

1. 新测试全部通过，且可重复稳定。

### P0-D6: Documentation and Migration Notes

交付：

1. `docs/USER_GUIDE.md` 增补安全模型与 session 语义。
2. `docs/plan/current_plan.md` 同步里程碑和验收门禁。
3. 变更日志模板（风险变更必须记录）。

退出条件：

1. 文档与行为一致，不存在“文档说有、实现没有”。

## 5. Strict Acceptance Gates (Blocking)

规则：以下 Gate 任一失败，P0-D 不得标记完成。

### Gate A: Correctness

1. `project_id` 计算稳定且可复现。
2. `resume/continue` 不跨项目串线。
3. 跨项目 `--session` 默认拒绝。

证据：

1. 对应单测与集成测试日志。
2. 失败场景截图或命令输出（可放 CI artifact）。

### Gate B: Security

1. `Critical` 命令不可被确认放行。
2. `external_directory` 越界访问触发 ask/deny。
3. UI 层绕过后（如 run/daemon）仍被统一网关拦截。

证据：

1. 工具入口测试覆盖（非 REPL 专属测试）。
2. Timeline 包含权限事件链路。

### Gate C: Observability

1. REPL 状态栏出现权限阻塞状态。
2. Session 面板出现结构化权限事件。
3. gRPC/SSE 同步输出同等字段。

证据：

1. interface 层 snapshot/映射测试。
2. SSE 订阅一致性测试。

### Gate D: Compatibility

1. 旧配置可启动（默认安全值生效）。
2. 新字段不导致旧客户端崩溃。

证据：

1. 回归测试与兼容策略文档更新。

## 6. Test Plan and Matrix

### 6.1 Unit

1. `project_identity_same_repo_same_id`
2. `project_identity_diff_repo_diff_id`
3. `project_identity_non_git_stable`
4. `boundary_external_directory_detected`
5. `risk_critical_always_denied`
6. `risk_high_requires_confirmation`

### 6.2 Integration

1. `continue_only_within_current_project`
2. `session_cross_project_denied_by_default`
3. `session_cross_project_allowed_with_explicit_flag`
4. `permission_gateway_enforced_for_run_and_repl`

### 6.3 E2E

1. `repl_timeline_contains_permission_events`
2. `grpc_sse_permission_fields_match_timeline`
3. `project_a_resume_returns_a_latest_root_session`

### 6.4 Mandatory Commands

1. `cargo check`
2. `cargo test -q`
3. `cargo test -q p0_d -- --nocapture`（新增测试统一前缀建议）

## 7. Risks and Rollback

主要风险：

1. 历史 session 无 `project_id` 导致继续会话歧义。
2. 权限默认值过严导致体验退化。

回滚策略：

1. 分阶段 feature flag（先写入字段、后开启强制校验）。
2. 对旧 session 提供迁移与回退日志，不做静默失败。

## 8. Definition of Done

P0-D 仅在以下全部满足后可标记完成：

1. `P0-D1 ~ P0-D6` 全部交付并通过 Gate A/B/C/D。
2. TODO 与 `current_plan` 同步更新并含可追踪链接。
3. 至少一次端到端演示证明：
   - A/B 项目隔离有效
   - 危险操作拦截有效
   - 权限事件实时可见
