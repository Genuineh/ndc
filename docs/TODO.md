# NDC TODO / Backlog

> 更新时间：2026-03-02（v18）  
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
| **P1-Scene** | ✅ 已完成 | repl.rs 模块化提取 + Scene 上下文自适应 TUI |
| **P1-TuiCrate** | ✅ 已完成 | TUI 独立 Crate 提取（ndc-tui） |
| **P1-TaskTodo** | ✅ 已完成 | Agent 驱动 TODO 规划流程（Task 系统集成） |
| **P1-Workflow** | 🔄 进行中 | TODO 驱动工作流重构（Pipeline 重新设计，Phase 1-5 已完成） |
| **P1** | 待开始 | 核心自治能力与治理 |
| **P2** | 待开始 | 多 Agent 与知识回灌体验 |

---

## 活跃工作

### P1-Scene: Context-Aware Adaptive Session TUI

> 设计文档：`docs/design/p1-scene-adaptive-tui.md`（v2 — 方案 A 精简）  
> 计划工期：6.5 工作日（4 Phase）

**目标**: 重构 `repl.rs`（5301 行）为 `tui/` 模块层次结构，添加轻量 Scene 渲染提示增强会话呈现。

| Phase | 状态 | 内容 | 天数 |
|-------|------|------|------|
| Phase 1 | ✅ 已完成 | repl.rs 提取为 `tui/` 模块（9 子模块，5301→268 行，153 测试分布于各模块） | 3 |
| Phase 2 | ✅ 已完成 | Scene 渲染提示（`tui/scene.rs` ~224 行，12 测试） | 0.5 |
| Phase 3 | ✅ 已完成 | 渲染增强（DiffPreview + 工具类型强调色，10 测试） | 2 |
| Phase 4 | ✅ 已完成 | 收尾（集成验证 + 文档 + 测试迁移） | 1 |

**关键决策**:
- Scene 是界面层内部细节（`tui/scene.rs`），不跨 crate
- decision crate 不变更 — 其价值在 P1 核心自治阶段兑现
- DiffPreview 按工具类型触发，不依赖 Scene
- 无 `/scene` 命令、无 feature flag、无额外 config

### 最近完成

#### P1-Scene: Context-Aware Adaptive Session TUI ✅

> 设计文档：`docs/design/p1-scene-adaptive-tui.md`（v2 — 方案 A 精简）

repl.rs（5301 行）重构为 `tui/` 模块层次结构（9 子模块、153 测试），添加 Scene 渲染提示与 DiffPreview 增强。

---

### P1-TuiCrate: TUI 独立 Crate 提取

> 设计文档：`docs/design/p1-tui-crate-extraction.md`  
> 前置：P1-Scene ✅

**目标**: 将 `crates/interface/src/tui/` 提取为独立 crate `ndc-tui`，实现干净的单向依赖图 `ndc-core ← ndc-tui ← ndc-interface`。

| Phase | 状态 | 内容 |
|-------|------|------|
| Phase 1 | ✅ 已完成 | 前置解耦：redaction 迁移至 ndc-core + 定义 AgentBackend trait |
| Phase 2 | ✅ 已完成 | Crate 创建与文件迁移（ndc-tui 骨架 + 代码搬迁 + 引用更新） |
| Phase 3 | ✅ 已完成 | Interface 适配（impl AgentBackend for AgentModeManager + repl.rs 更新） |
| Phase 4 | ✅ 已完成 | 验证与清理（全量测试 + clippy + 文档同步） |

**关键设计决策**:
- `redaction`（117 行）迁入 ndc-core（仅需 regex，无业务耦合）
- `AgentBackend` trait 定义在 ndc-tui 中，ndc-interface 实现（依赖反转，消除循环依赖）
- DTO 类型（AgentStatus / ProjectCandidate 等）定义在 ndc-tui，与 interface 类型 field-by-field 映射
- `pub(crate)` → `pub` 可见性升级

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

### P1-TaskTodo: Agent 驱动 TODO 规划流程

> 前置：P1-TuiCrate ✅  
> 预计 Phase：5 Phase  
> 关键设计文档：待创建 `docs/design/p1-task-todo-planning.md`

**目标**: 用户输入任务描述后，Agent 自动进行 planning 并产生 TODO 列表，使用 NDC 既有 Task 系统持久化维护，按 project/session 隔离，在 TUI 当前会话中持久展示，支持状态跟踪与完成标记。

#### 核心设计决策

**1. 复用 Task 模型 vs 新建轻量模型**

复用现有 `Task` 结构体（`crates/core/src/task.rs`），但以"精简模式"使用：
- `intent` / `verdict` / `quality_gate` / `snapshots` 保持 `None`（这些是重量级编排字段）
- 仅使用 `id` / `title` / `description` / `state` / `metadata`（含 `tags`、`priority`）
- `metadata.tags` 中注入 `project:<project_id>` 和 `session:<session_id>` 标签用于隔离筛选
- **理由**: 避免引入新类型导致 Storage trait 膨胀，同时保持与未来 orchestrator 编排的兼容性

**2. 项目/会话隔离策略**

- Task 的 `metadata.tags` 存储隔离标签：`["project:<project_id>", "session:<session_id>"]`
- Storage trait 扩展 `list_tasks_by_tags(tags: &[String]) -> Result<Vec<Task>, String>` 方法
- SQLite 实现通过 JSON 字段查询（tags 已序列化为 JSON array）
- MemoryStorage 内存过滤
- 删除行为：不物理删除，标记 `Cancelled` 状态即可

**3. TUI 持久展示方案 — 右侧边栏**

在会话主区域右侧新增可折叠 TODO 侧边栏，采用水平分割布局：

```
┌─────────────────────────────────────────────────────────┐
│ [0] Title Bar                                           │
│ [1] Workflow Progress                                   │
├───────────────────────────────────┬─────────────────────┤
│                                   │  📋 TODO (3/7)      │
│  [2] Conversation Body            │ ─────────────────── │
│  (弹性填充)                       │  ✓ 1. 数据库迁移   │
│                                   │  ▶ 2. 编写测试     │
│                                   │  ☐ 3. 用户认证     │
│                                   │  ☐ 4. API 接口     │
│                                   │  ...还有 3 项       │
├───────────────────────────────────┴─────────────────────┤
│ [3] Permission Bar (条件)                                │
│ [4] Status Hint                                         │
│ [5] Input Area                                          │
└─────────────────────────────────────────────────────────┘
```

- TODO 侧边栏在 Conversation Body 右侧，与会话内容水平并排
- 仅分割 Conversation Body 行区域，Title/Workflow/StatusHint/Input 保持全宽
- 侧边栏宽度：固定 `Constraint::Length(28)` 字符（紧凑显示标题足够）
- 折叠时侧边栏宽度为 0，Conversation Body 占满全宽
- 默认展开，无 TODO 任务时自动折叠
- 快捷键 `Ctrl+T` 切换折叠/展开
- 显示格式：`[状态图标] 序号. 标题`（单行紧凑），标题超宽截断加 `…`
- 顶部标题行显示完成进度：`📋 TODO (已完成/总数)`
- 列表可滚动，超出面板高度时底部显示 `...还有 N 项`

**4. Agent Planning 流程**

```
用户输入 → Agent 识别需要规划（含 /plan 命令或自动检测）
    → Agent 调用 planning 工具
    → 产生结构化 TODO 列表（JSON）
    → 逐条创建 Task（带 project/session tags）
    → 持久化到 Storage
    → TUI 刷新 TODO Panel 显示
    → Agent 按序执行，完成后标记状态
```

- `/plan <描述>` — 显式触发规划，Agent 分析描述并生成 TODO 列表
- `/todo` — 查看当前会话 TODO 列表
- `/todo done <编号>` — 手动标记某项完成
- `/todo add <标题>` — 手动添加单条 TODO
- Agent 自动模式下，完成子任务后自动调用 `complete_task` 更新状态

#### 分 Phase 实施计划

| Phase | 内容 | 预估 |
|-------|------|------|
| Phase 1 | ✅ Core + Storage 扩展（Task tags 隔离 + list_tasks_by_tags, 17+1 测试） | 1 天 |
| Phase 2 | ✅ AgentBackend trait 扩展（TodoItem/TodoState DTO + 5 CRUD 方法 + interface 实现） | 1 天 |
| Phase 3 | ✅ TUI TODO 右侧边栏（todo_panel.rs + layout split + Ctrl+O 切换 + 自动刷新, 6 测试） | 1.5 天 |
| Phase 4 | ✅ 命令系统集成（/plan /todo 命令 + SlashCommandSpec 自动补全） | 1.5 天 |
| Phase 5 | ✅ 端到端集成（TODO 刷新 + 文档收尾） | 1 天 |

---

#### Phase 1: Core + Storage 扩展

**ndc-core 变更**:
- `task.rs` — 新增辅助方法：
  ```rust
  impl Task {
      /// 创建 TODO 任务（轻量模式，含 project/session 隔离标签）
      pub fn new_todo(
          title: String,
          description: String,
          project_id: &str,
          session_id: &str,
      ) -> Self { ... }

      /// 检查是否匹配指定 tags
      pub fn has_tags(&self, required: &[String]) -> bool { ... }

      /// 快捷标记完成
      pub fn mark_completed(&mut self) -> Result<(), TransitionError> { ... }
  }
  ```

- `task.rs` — `TaskState` 新增 `Display` impl（TUI 渲染用）

**ndc-storage 变更**:
- `trait_.rs` — Storage trait 新增方法：
  ```rust
  async fn list_tasks_by_tags(&self, tags: &[String]) -> Result<Vec<Task>, String>;
  ```
- `sqlite.rs` — SQLite 实现 `list_tasks_by_tags`（JSON `tags` 字段 LIKE 查询）
- `memory.rs` — MemoryStorage 实现 `list_tasks_by_tags`（内存过滤）

**测试（Red→Green）**:
- `Task::new_todo` 创建行为（含正确 tags）
- `Task::has_tags` 过滤逻辑
- `Task::mark_completed` 状态转换（从各种初始状态）
- Storage `list_tasks_by_tags` 隔离正确性
- 跨 project 隔离 / 跨 session 隔离

---

#### Phase 2: AgentBackend trait 扩展

**ndc-tui `agent_backend.rs` 变更**:
- 新增 DTO：
  ```rust
  /// TODO 任务的轻量视图（TUI 显示用）
  #[derive(Debug, Clone)]
  pub struct TodoItem {
      pub id: String,          // TaskId 的字符串形式
      pub index: usize,        // 会话内序号（1-based，方便用户引用）
      pub title: String,
      pub state: TodoState,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum TodoState {
      Pending,
      InProgress,
      Completed,
      Failed,
      Cancelled,
  }
  ```

- AgentBackend trait 新增方法：
  ```rust
  /// 获取当前会话的 TODO 列表
  async fn list_session_todos(&self) -> anyhow::Result<Vec<TodoItem>>;

  /// 创建 TODO（返回新建的 TodoItem）
  async fn create_todo(&self, title: &str, description: &str) -> anyhow::Result<TodoItem>;

  /// 批量创建 TODO（用于 Agent planning 输出）
  async fn create_todos(&self, items: Vec<(String, String)>) -> anyhow::Result<Vec<TodoItem>>;

  /// 更新 TODO 状态（按会话内序号）
  async fn update_todo_state(&self, index: usize, state: TodoState) -> anyhow::Result<()>;

  /// 标记 TODO 完成（按会话内序号）
  async fn complete_todo(&self, index: usize) -> anyhow::Result<()>;
  ```

**ndc-interface `agent_backend_impl.rs` 变更**:
- 实现上述 5 个新方法
- `list_session_todos`: 调用 `storage.list_tasks_by_tags(&["project:<id>", "session:<id>"])`，映射为 `TodoItem`
- `create_todo` / `create_todos`: 调用 `Task::new_todo()`，保存到 Storage
- `update_todo_state` / `complete_todo`: 查找 Task → `request_transition()` → 保存

**测试（Red→Green）**:
- AgentBackend impl 的 CRUD 测试
- 批量创建正确性
- 状态更新联动 Storage 持久化
- 序号索引的正确映射

---

#### Phase 3: TUI TODO 右侧边栏

**布局改动**（`layout_manager.rs`）:
- `tui_layout_constraints()` 签名不变（垂直层级不变）
- 新增 `tui_session_split(area: Rect, show_todo: bool) -> (Rect, Option<Rect>)` 函数
  - 在 `app.rs` 渲染时，对 Conversation Body 所在的 `chunks[2]` 做水平二分
  - `show_todo = true` 时：`Layout::horizontal([Constraint::Min(30), Constraint::Length(28)])`
  - `show_todo = false` 时：全部给 Conversation Body，返回 `(full_area, None)`
- 侧边栏宽度 28 字符，留给会话区至少 30 字符保证可读
- 终端宽度 < 60 时自动折叠侧边栏（空间不足）

**渲染**（新文件 `todo_panel.rs`）:
- `render_todo_sidebar(frame, area: Rect, items: &[TodoItem], scroll_offset: usize)`
- 顶部标题行：`📋 TODO (2/5)` — 显示已完成/总数，带 `Block::bordered()` 边框
- 列表区域：`[图标] 序号. 标题`，标题超过面板宽度时截断加 `…`
- 状态图标映射：`Pending→☐  InProgress→▶  Completed→✓  Failed→✗  Cancelled→⊘`
- 颜色：Pending=白, InProgress=黄, Completed=绿(dimmed), Failed=红, Cancelled=灰
- 已完成项排到列表底部（视觉降优先级）
- 超出可视高度时底部显示 `...还有 N 项`

**状态管理**（`lib.rs` `ReplVisualizationState`）:
- 新增 `show_todo_panel: bool`（默认 `true`）
- 新增 `todo_items: Vec<TodoItem>`（TUI 侧缓存）
- 新增 `todo_scroll_offset: usize`（侧边栏滚动偏移）

**输入处理**（`input_handler.rs`）:
- `Ctrl+T` → 切换 `show_todo_panel`
- TODO 侧边栏不接受焦点，仅被动显示

**事件刷新机制**:
- 每次 Agent 完成一轮对话后，TUI 主循环调用 `backend.list_session_todos()` 刷新
- 收到 TODO 变更事件时立即刷新（通过已有的 `AgentSessionExecutionEvent` 扩展）

**测试（Red→Green）**:
- `tui_session_split` 水平分割正确性（展开/折叠/窄终端自动折叠）
- `render_todo_sidebar` 渲染输出验证（标题行、图标、截断、排序）
- 状态图标映射
- `Ctrl+T` 折叠/展开切换
- 滚动偏移与 `...还有 N 项` 省略显示
- 终端宽度 < 60 时自动隐藏

---

#### Phase 4: 命令系统集成

**新增斜杠命令**:
- `/plan <描述>` — 提交给 Agent 进行规划，Agent 返回结构化 TODO 列表
- `/todo` — 列出当前会话 TODO（等价于手动刷新 TODO Panel）
- `/todo add <标题>` — 手动添加单条 TODO
- `/todo done <序号>` — 标记指定序号 TODO 为完成
- `/todo remove <序号>` — 标记指定序号为取消

**命令处理**（`input_handler.rs` 或 `app.rs` 的命令分发）:
- `/plan` 走 `process_input` 但附加 system prompt 指引 Agent 输出 JSON 计划
- `/todo` 系列直接操作 AgentBackend 方法

**Agent Planning 工具**（`crates/runtime/src/tools/`）:
- 新增 `planning_tool.rs`：Agent 可调用的工具，功能包括：
  - `create_plan(items: Vec<{title, description}>)` — 批量创建 TODO
  - `update_plan_item(index, state)` — 更新单项状态
  - `get_current_plan()` — 获取当前 TODO 列表
- 在 Agent system prompt 中注入 planning 工具的使用指引
- 工具权限：`task_manage` 类别，默认 `Allow`（已在 SEC-H3 中配好）

**测试（Red→Green）**:
- `/plan` 命令解析与 Agent 调用
- `/todo` 子命令解析
- Planning 工具的 CRUD 行为
- Agent 自动完成任务时状态联动

---

#### Phase 5: 端到端集成与收尾

**自动规划检测**:
- Agent 在 system prompt 中被告知：面对复杂任务时应先使用 planning 工具生成 TODO
- Agent 自动调用 `create_plan` 工具创建任务列表
- Agent 逐项执行，每完成一项调用 `update_plan_item` 标记

**Session 恢复**:
- 恢复已有 Session 时（`/session use <id>`），自动从 Storage 加载该 session 的 TODO 列表
- 跨 session 的 TODO 不会互相干扰

**状态持久化验证**:
- 关闭 TUI → 重新打开 → 切换回同一 project/session → TODO 列表完整恢复
- 状态变更即时持久化到 SQLite

**文档更新**:
- `docs/USER_GUIDE.md` — 新增 TODO/Planning 功能说明
- `docs/design/p1-task-todo-planning.md` — 完整设计文档
- `CLAUDE.md` — 必要时更新

**测试（Red→Green）**:
- 端到端：创建 → 展示 → 执行 → 完成 → 持久化 → 恢复
- Session 隔离：两个 session 的 TODO 互不可见
- Project 隔离：两个 project 的 TODO 互不可见
- 大量 TODO 的性能测试（100+ items）

---

### P1-Workflow: TODO 驱动工作流重构

> 前置：P1-TaskTodo ✅  
> 设计文档：`docs/design/p1-workflow-todo-driven.md`  
> 预计 Phase：6 Phase

**背景**: 当前 5 阶段 Pipeline（Planning→Discovery→Executing→Verifying→Completing）是以 LLM 单轮对话为中心的线性流程，无法自动产生 TODO、无法围绕 TODO 进行结构化执行。需重构为 **TODO 驱动**的工作流，让 TODO 成为工作流的核心编排单元。

**新工作流 Pipeline（8 阶段）**:

```
LoadContext → Compress → Analysis → Planning → [TodoLoop] → Review → Report
                                        │
                                        ▼
                                  ┌───────────┐
                                  │ Per-TODO:  │
                                  │ Classify → │──→ Coding: Test→Code→Regress→Doc
                                  │            │──→ Normal: Execute→Test→Doc
                                  │ Review →   │
                                  │ MarkDone   │
                                  └───────────┘
```

| 阶段 | 索引 | 职责 |
|------|------|------|
| **LoadContext** | 1 | 加载工具清单、Skills、MCP 能力、项目记忆、会话历史 |
| **Compress** | 2 | 上下文超限时压缩摘要（可跳过） |
| **Analysis** | 3 | 结合上下文分析用户需求，产出需求理解文档 |
| **Planning** | 4 | 将需求分解为 TODO 列表，写入 Task 系统（**必须产生 TODO**） |
| **Executing** | 5 | 围绕 TODO 执行循环：场景判断→编码/普通路径→单项 Review→标记完成 |
| **Verifying** | 6 | 所有 TODO 完成后，全局回归验证 |
| **Completing** | 7 | 文档收尾、知识回灌 |
| **Reporting** | 8 | 生成执行报告（变更摘要、测试结果、TODO 完成率） |

**执行阶段场景分类**:

- **编码场景**（文件变更类 TODO）: TDD 红绿循环
  1. 先写失败测试（Red）
  2. 最小实现通过测试（Green）
  3. 回归测试确保不破坏
  4. 更新相关文档
- **普通场景**（配置、调研、文档类 TODO）:
  1. 执行任务
  2. 验证结果
  3. 更新文档

#### 分 Phase 实施计划

| Phase | 内容 | 预估 |
|-------|------|------|
| Phase 1 | ✅ Core 模型扩展：`AgentWorkflowStage` 8 阶段 + `TodoExecutionScenario` 枚举 + Scene 映射更新 | 1 天 |
| Phase 2 | ✅ ConversationRunner 前 4 阶段方法：LoadContext→Compress→Analysis→Planning | 2 天 |
| Phase 3 | ✅ TODO 执行循环：Per-TODO Classify→Execute→Review→MarkDone + TDD 路径 | 2 天 |
| Phase 4 | ✅ Verifying + Completing + Reporting 阶段实现 | 1 天 |
| Phase 5 | ✅ TUI 适配：Workflow Progress Bar 更新 + TODO 状态实时联动 + Scene 映射 | 1 天 |
| Phase 6 | 端到端测试 + 文档收尾 | 1 天 |

#### Phase 1: Core 模型扩展 ✅ `d4f56fb`

- **AgentWorkflowStage**: 5 阶段 → 8 阶段（LoadContext/Compress/Analysis/Planning/Executing/Verifying/Completing/Reporting）
- **新增类型**: `TodoExecutionScenario`(Coding/Normal/FastPath)、`ContextSnapshot`、`AnalysisResult`
- **新增事件**: 6 个 `AgentExecutionEventKind` 变体（TodoStateChange/AnalysisComplete/PlanningComplete/TodoExecutionStart/TodoExecutionEnd/Report）
- **Scene 映射**: `classify_scene()` 更新支持所有 8 阶段（load_context/compress/analysis→Analyze, reporting→Review）
- **Progress Bar**: `WORKFLOW_STAGE_ORDER` 更新为 8 条目，百分比计算适配
- **Match exhaustiveness**: `event_renderer.rs` + `chat_renderer.rs` 新事件类型覆盖
- **测试**: +13 新测试，全部 GREEN

#### Phase 2: ConversationRunner 前 4 阶段 ✅ `02e9995`

- **`estimate_context_tokens()`**: ~4 chars/token 粗估算法
- **`load_context()`**: 收集工具数量 + token 估算 → `ContextSnapshot`
- **`compress_context()`**: 超 32K token 阈值时裁剪消息，否则跳过
- **`run_analysis_round()`**: 独立 LLM 调用 → JSON 解析为 `AnalysisResult`（含降级回退）
- **`run_planning_round()`**: 独立 LLM 调用 → `Vec<String>` TODOs（≥1 保证，空输出自动兜底）
- **测试**: +9 新测试（token 估算/load_context/compress 条件/analysis JSON/planning 正常+空输出回退），全部 GREEN

#### Phase 3: TODO 执行循环 ✅

- **`classify_scenario()`**: 关键词匹配判断场景（implement/refactor/fix/add test/write/bug → Coding，其余 → Normal，FastPath 透传）
- **`run_rounds_with_context()`**: 可复用 LLM 循环 — 注入 context prompt 为 system message，执行 LLM 轮次 + 工具调用
- **`execute_single_todo()`**: 完整单 TODO 生命周期 — emit TodoExecutionStart → classify_scenario → 构建 TDD/Normal/FastPath prompt → run_rounds_with_context → emit TodoExecutionEnd
- **测试**: +6 新测试（classify_scenario 3 个场景 + run_rounds_with_context + execute_single_todo 事件 + TDD prompt），全部 GREEN

#### Phase 4: Verifying + Completing + Reporting ✅

- **`run_global_verification()`**: emit Verifying stage → LLM 全局回归验证，汇总所有 TODO 完成状态
- **`run_completion()`**: emit Completing stage → LLM 文档收尾 + 知识回灌
- **`generate_execution_report()`**: emit Reporting stage → LLM 生成执行报告（TODO 完成率 + 变更摘要 + 测试结果）→ emit Report 事件
- **测试**: +3 新测试（verification/completion/report 各 1，验证 stage 事件），全都 GREEN
- **总计**: conversation_runner 测试 24 个（原始 3 + Phase 2 9 + Phase 3 6 + Phase 4 3），全部 GREEN

#### Phase 5: TUI 适配 ✅

- **event_renderer.rs**: 6 个新事件类型渲染（TodoExecutionStart/End → `[TODO Start/Done]`, AnalysisComplete → `[Analysis]`, PlanningComplete → `[Plan]`, Report → `[Report]`, TodoStateChange → sidebar dirty flag）
- **chat_renderer.rs**: 同 6 个事件类型产生 ChatEntry（SystemNote/StageNote），不再 no-op
- **app.rs**: `todo_sidebar_dirty` 检测 → 实时刷新 TODO sidebar（不再仅在 session 结束后）
- **lib.rs**: `ReplVisualizationState` 新增 `todo_sidebar_dirty: bool` 字段
- **测试**: +10 新测试（event_renderer 6 + chat_renderer 4），全部 GREEN
- **总计**: ndc-tui 测试 170 个，全部 GREEN

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
