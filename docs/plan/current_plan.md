# NDC 架构排查与重规划（2026-02-12）

> 最新同步：2026-02-25  
> 当前阶段：`P0-D`（安全边界与项目级会话隔离）  
> 上一阶段：`P0-C`（Workflow-Native REPL 与实时可观测）已完成

## 当前快照（2026-02-25）

1. 工程治理重构已完成：
   - 移除 8 个空占位 crate 目录（cli, context, daemon, execution, observability, plugins, repl, task）
   - 从 runtime 抽取独立 `ndc-storage` crate（Storage trait + MemoryStorage + SqliteStorage）
   - 全 workspace 统一 Rust edition 2024（`edition.workspace = true`）
2. P0-C 已完成并通过核心回归：
   - REPL/gRPC/SSE 统一 workflow + token 可观测语义
   - `/workflow compact|verbose`、timeline replay、订阅一致性测试已落地
2. 稳定性修复已补齐：
   - MiniMax 别名 provider 的配置凭证查找回退已修复（支持 `minimax` 键）
   - `ndc_task_update` 非法状态迁移已改为严格拒绝（不再强制覆盖状态）
   - `Executor` intent 执行路径已修复步骤丢失（去除 `task.clone()` 误用）
   - 运行时安全边界已支持 `working_dir/project_root` 根提示，减少 `external_directory` 误判
3. P0-D 已启动并完成首批落地：
   - `ProjectIdentity` + session 项目元数据 + REPL 项目标识已接入
   - `orchestrator` 项目会话索引与最近会话游标已接入
   - REPL `/new`、`/resume` 与 CLI `run --continue/--session`（含跨项目默认拒绝）已接入
   - daemon/gRPC timeline 会话校验已对齐到同一 session 归属语义（同项目可切换，跨项目默认拒绝）
   - runtime 工具主链已接入首批统一权限网关（shell/fs/git/read/write/edit/list/glob/grep）
   - runtime `ask` 已落地 REPL 确认重试闭环（`requires_confirmation` + 单次授权覆盖）
   - REPL 状态栏新增权限可观测字段：`perm_state/perm_type/perm_risk`
   - orchestrator 已接入权限事件闭环：`permission_asked -> permission_approved/rejected`
   - gRPC/SSE 首批一致性回归已补齐：权限生命周期事件映射/序列化与 replay message 一致性断言
   - 非交互通道确认策略已落地：无 TTY 场景不阻塞 stdin，返回 `non_interactive confirmation required`
   - REPL 项目导航已落地：启动展示当前项目与最近会话，支持 `/project status|list|pick|use|sessions`
   - TUI 项目选择已强化：`Ctrl+P` 直达选择器，active 项优先展示并带状态标记
   - 跨进程项目索引已持久化：`discover/known_project_ids` 可在新进程恢复已知项目
   - 跨进程会话归档已持久化：`enable` 启动会 hydrate 已归档会话并恢复当前项目最近 session（含 timeline）
   - `process_input` 成功后会回写 session 快照，支持重启后的多轮上下文与 timeline 连续性
   - 项目切换执行上下文已补齐：`shell/fs` 使用当前项目 `working_dir`，并同步 `Project Context` 提示
4. 下一步：
   - 执行 P0-D Gate A/B/C/D 全量验收回归并归档证据
   - P1-UX：REPL TUI 布局与体验重设计（详见 `docs/design/p1-repl-ux-redesign.md`）

## 0. 基础愿景（统一口径）

NDC 的目标不是“再造一个 CLI 工具集”，而是：

1. 对用户提供接近 OpenCode 的交互体验：自然语言驱动、工具协作、可连续会话。
2. 对 Agent 提供 NDC 内置工程系统：任务系统、知识/记忆系统、质量门禁、测试与验收闭环。

也就是：**OpenCode 风格交互壳 + NDC 工程内核**。

---

## 1. 对照 `ENGINEERING_CONSTRAINTS.md` 的现状结论

### P0（必须优先修）

- 任务工具链名义存在、实际不可落库（`ndc_task_*` 之前为 mock）。
- Agent 调用工具与任务验证未共享同一存储，导致“能创建但无法验证”的割裂。
- 会话历史没有稳定回写，连续对话上下文容易丢失。
- 权限规则未真正进入工具执行入口（配置了 `allow/ask/deny`，执行时未生效）。

### P1（主链已具备但仍需增强）

- `run/repl -> agent -> tool-calling` 主链已打通。
- OpenAI/OpenRouter function-calling 已可用。
- Discovery/WorkingMemory/Invariant/Telemetry 结构代码存在，但与 agent 主循环还未完成全量闭环接入。

---

## 2. 本轮“回到正轨”已落地调整

### 2.1 任务系统从 mock 变为真实工具链

- `ndc_task_create`：真实创建并写入存储。
- `ndc_task_list`：真实读取并支持 state/priority/created_by/search 过滤。
- `ndc_task_update`：真实更新状态、优先级、标签、备注并落库。
- `ndc_task_verify`：基于真实任务状态/步骤执行验证。

实现位置：

- `crates/runtime/src/tools/ndc/task_create.rs`
- `crates/runtime/src/tools/ndc/task_list.rs`
- `crates/runtime/src/tools/ndc/task_update.rs`
- `crates/runtime/src/tools/ndc/task_verify.rs`

### 2.2 工具装配统一为“共享存储注入”

- 新增：
  - `create_default_tool_manager_with_storage`
  - `create_default_tool_registry_with_storage`
- 默认工具集已注册 `ndc_task_*`。
- CLI/REPL/Executor 统一使用同一份 storage 注入，避免状态分叉。
- 存储抽象已独立为 `ndc-storage` crate，runtime 通过依赖复用。

实现位置：

- `crates/storage/src/trait_.rs`（Storage trait 定义）
- `crates/storage/src/memory.rs`（MemoryStorage 实现）
- `crates/storage/src/sqlite.rs`（SqliteStorage 实现，feature = "sqlite"）
- `crates/runtime/src/tools/mod.rs`
- `crates/runtime/src/executor.rs`
- `crates/runtime/src/lib.rs`（re-exports from ndc-storage）
- `crates/interface/src/cli.rs`
- `crates/interface/src/repl.rs`

### 2.3 Agent 验证与工具执行存储对齐

- `TaskVerifier` 改为读取 runtime shared storage（通过 adapter）。
- 解决“工具创建任务，但 verifier 看不到”的问题。

实现位置：

- `crates/interface/src/agent_mode.rs`

### 2.4 权限规则接入工具执行入口

- `ReplToolExecutor` 接入权限分类与 `allow/ask/deny` 判定。
- 对 `Ask` 场景增加终端确认（可通过 `NDC_AUTO_APPROVE_TOOLS=1` 自动放行）。
- 覆盖典型分类：`file_read/file_write/file_delete/git_commit/shell_execute/network/task_manage`。

实现位置：

- `crates/interface/src/agent_mode.rs`

### 2.5 会话历史真正回写

- Agent 主循环和流式入口均将用户/助手/工具消息回写 session。
- 会话可持续承接，减少“每轮重新理解”的退化。

实现位置：

- `crates/core/src/ai_agent/orchestrator.rs`

---

## 3. 约束阶段对齐矩阵（0-9）

| 阶段 | 约束目标 | 当前状态 |
|---|---|---|
| 0 Lineage | 任务谱系继承 | 部分实现（injector 有，主循环继承策略待统一） |
| 1 Understand | 知识/TODO 检索理解 | 部分实现（基础 prompt+tools 已有，知识库检索深度待增强） |
| 2 Decompose | 原子分解与任务链 | 部分实现（任务工具可用，自动分解策略待收口） |
| 3 Discovery | 影响面+硬约束 | 结构已实现，尚未成为 agent 执行前强制阶段 |
| 4 WorkingMemory | Abstract/Raw/Hard | 结构已实现，尚未全量注入每轮执行 |
| 5 Develop | 子任务执行+重试 | 已有主循环，失败分类与策略化重试待增强 |
| 6 Accept | 质量门禁+回归验收 | 基础 verifier 可用，硬约束驱动回归待接入 |
| 7 Failure→Invariant | 失败归因与固化 | 有基础模块，TTL/version 冲突治理待串联 |
| 8 Document | 文档自动回灌 | 有 doc updater 基础，触发策略待完善 |
| 9 Complete | 完成与遥测 | 基础结构在，统一指标采集待落地 |

---

## 4. 新的工程主链（统一方案）

```text
User (run/repl)
  -> AgentModeManager
    -> AgentOrchestrator (session/tool loop)
      -> ToolRegistry (default + ndc_task + extensions)
        -> Runtime (tools, workflow, quality)
          -> Storage (trait-based: MemoryStorage / SqliteStorage)
          -> Verification + Knowledge Feedback
```

关键原则：

1. 单一执行主链：所有用户输入都走 `agent -> tools -> verify`。
2. 单一状态真相：任务工具、执行器、验证器共享 storage。
3. 单一权限入口：工具执行前统一判定 `allow/ask/deny`。
4. 单一文档口径：文档只描述“已可运行能力 + 明确待办”。

---

## 5. 下一阶段（必须完成）

### Phase A（短期，P0）- 已完成（2026-02-12）

1. Discovery 结果（Hard Constraints）已强制注入 QualityGate（执行阶段强制检查合并）。
2. WorkingMemory（Abstract/Raw/Hard）已注入 orchestrator prompt 主循环路径。
3. 主链 smoke test 已补齐：
   - 覆盖 `ndc_task_create -> ndc_task_update -> ndc_task_verify`
   - 覆盖文件工具调用 + 会话续接

### Phase A.5（短期收口）

1. Discovery 失败策略分级已落地（`degrade`/`block`，支持配置与环境变量）。
2. WorkingMemory 数据已接入任务系统/知识库真实源（活跃任务 + memory 访问记录），减少会话推断依赖。
3. 主链 smoke 已覆盖 QualityGate 失败反馈闭环与权限交互分支。

### Phase A.6（短期稳态）

1. 统一 Hard Invariants 类型定义已完成（`WorkingMemory` 与 `GoldMemory` 共享 `InvariantPriority`）。
2. 文本化约束注入已升级为结构化 Hard Invariants 注入。

### Phase A.7（短期闭环）

1. `ai_agent/injectors/invariant.rs` 与 core memory invariant 优先级语义收敛已完成。
2. Hard Invariants 与 Discovery/QualityGate 回灌闭环已接入（失败后固化、成功后验证计数）。

### Phase A.8（短期工程化）

1. GoldMemoryService 与 runtime storage 打通已完成，支持持久化与会话间复用。
2. Discovery/QualityGate 失败原因结构化入库，避免仅字符串摘要。

### Phase A.9（短期稳固）

1. GoldMemory 持久化 schema 版本化与迁移策略设计（`v1` -> 后续版本）。
2. Discovery/QualityGate 结构化事实映射已接入 GoldMemory（规则、来源、验证证据）。

### Phase A.10（短期增强）

1. GoldMemory 持久化 schema 版本化与迁移策略已落地（`v1` 兼容读取，写回 `v2`）。
2. 将 Discovery 执行阶段结构化信号直接回灌 GoldMemory（不仅依赖验证阶段）。

### Phase A.11（短期完善）

1. Discovery 执行阶段结构化信号已直接回灌 GoldMemory（规则、证据、优先级）。
2. GoldMemory schema 迁移审计元数据增强（迁移来源/时间/触发上下文）。

### Phase A.12（短期收束）

1. GoldMemory schema 迁移审计元数据已落地（字段与写入时机统一）。
2. Discovery/Verifier 双源事实统一去重与冲突合并策略。

### Phase A.13（短期提效）

1. Discovery/Verifier 双源事实统一去重与冲突合并策略已实现（`upsert_system_fact`）。
2. GoldMemory 事实检索接口与工具化已接入（`ndc_memory_query` 支持 tags/priority/source）。

### Phase A.14（短期收尾）

1. 将 GoldMemory 检索结果接入 orchestrator 的自动上下文选择与 Top-K 注入。
2. 增加检索结果与执行质量的关联指标（为 Telemetry 阶段做铺垫）。

### Phase A.15（短期主线：Workflow-Native REPL）

Status（2026-02-25）：
1. 已完成第一批：orchestrator 阶段事件发射 + REPL workflow 状态区（stage/stage_ms/blocked）。
2. 已落地阶段单一语义：core `AgentWorkflowStage` 枚举作为 workflow stage 真相源。
3. 已落地阶段分组视图：Session 与 `/timeline` 支持 `[stage:<name>]` 分段展示。
4. 已新增 `/workflow compact|verbose` 双视图与参数补全提示。
5. 已补齐阶段耗时边界：当前阶段 `total_ms` 累计 `active_ms`，并在历史缓存达到上限时提示统计可能不完整。
6. 已补齐结构化阶段载荷：`AgentExecutionEvent` 增加 `stage/detail/index/total` 显式字段，REPL/gRPC 优先读取结构化字段。
7. 已新增综合 e2e（orchestrator）：多轮 + 多次 tool call + permission + timeline replay + workflow/token 断言。
8. 已新增 interface 侧结构化字段测试：REPL workflow 渲染与 gRPC workflow 映射均覆盖结构化载荷路径。
9. 已新增订阅端一致性 e2e：`SubscribeSessionTimeline` 与 `GetSessionTimeline` 在 replay 事件上的 workflow/token 字段一致。
10. 已完成稳定性修复：MiniMax 别名配置凭证回退与 `ndc_task_update` 非法迁移拒绝（状态机约束恢复）。

1. 以 NDC 内部 workflow 语义驱动 REPL 展示（Planning/Discovery/Executing/Verifying/Completing）。
2. 在 orchestrator 主循环中发射阶段切换事件，并保证多轮会话可恢复当前阶段。
3. REPL 状态区展示“当前阶段 + 阶段进度 + 阶段耗时 + 阻塞状态”。

### Phase A.16（短期主线：Token 可观测）

Status（2026-02-24）：
1. 已完成第一批：每轮 LLM usage 采集（provider 优先，缺失回退 estimate）。
2. 已完成第一批：REPL/gRPC/SSE 统一输出 token 指标（round + session）。
3. 已完成第一批：ExecutionEvent 协议新增 `token_*` 字段并保持兼容降级策略。

1. 在每轮 LLM 调用后采集 usage（优先 provider 返回，缺失回退 estimate）。
2. 将 token 指标纳入 execution timeline（本轮 + 会话累计）。
3. REPL / gRPC / SSE 对齐展示 token 指标，形成统一观测口径。

### Phase B（中期，P1）

1. 失败分类自动化（Logic/TestGap/SpecConflict/NonDeterministic）接入重试决策。
2. Invariant 的 TTL/version 生效与冲突检测接入执行前检查。
3. Telemetry 指标落地：autonomous_rate / intervention_cost / token_efficiency。

### Phase C（中期，P1）

1. MCP/Skills 纳入默认工具发现链与权限治理链。
2. 文档自动回灌（阶段 8）和发布前校验流程打通。

---

## 6. 验收基线（回到正轨判定）

每次架构改动必须同时满足：

1. `cargo check` 通过。
2. `cargo test -q` 通过。
3. 至少一条主链 smoke 用例通过：`run/repl -> tool-calling -> verify -> session persisted`。
4. 文档与代码保持一致：能力上线即更新 `docs/TODO.md` 与本文件。
