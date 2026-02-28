# P1-Scene: Context-Aware Adaptive Session TUI

> 创建日期：2026-02-27  
> 修订日期：2026-02-28（v5 — 全部完成）  
> 状态：✅ 已完成  
> 前置依赖：P0-SEC 全部完成、P1-UX 全部完成  
> 关联文档：`docs/TODO.md` · `docs/plan/current_plan.md`

## 概述

重构 `repl.rs`（5301 行）为 `tui/` 模块层次结构，然后添加轻量的 Scene 渲染提示增强会话呈现。**Scene 定位为界面层内部实现细节**（`tui/scene.rs`，~50 行辅助函数），不跨 crate、不暴露命令、不加 feature flag。Diff 预览作为独立特性按工具类型触发。

### 方案 A 定位决策（2026-02-28）

经重新评估 Scene 的作用与边界后决定：

1. **decision crate 是孤岛**：`DecisionEngine` 完整实现但整个 workspace 无调用者。Scene 不应叠加在断连架构上。decision crate 的价值应在 P1（核心自治：工具执行授权）阶段兑现。
2. **AgentWorkflowStage 已覆盖 80% 的 Scene 功能**：标题栏进度条、阶段着色、按 EventKind+Verbosity 差异渲染均已实现。Scene 本质是 `WorkflowStage + tool_name → 渲染提示` 的 ~30 行映射。
3. **精简方案**：Scene 放 `tui/scene.rs` 作为 `chat_renderer` 内部辅助；砍掉 `/scene` 命令、手动覆盖、feature flag；DiffPreview 按工具类型直接触发。

## 最终状态

- `repl.rs`：268 行（ReplConfig + ReplState + run_repl + 4 测试），从 5301 行精简至此
- 已提取 9 个子模块至 `tui/` 目录（~7913 行代码+测试）：
  - `tui/app.rs`（536L）— TUI 主事件循环
  - `tui/chat_renderer.rs`（1952L）— ChatEntry 模型 + 样式 + DiffPreview + 42 测试
  - `tui/commands.rs`（1084L）— 斜杠命令路由 + 显示
  - `tui/event_renderer.rs`（1582L）— 事件渲染 + append 辅助 + 32 测试
  - `tui/input_handler.rs`（791L）— 输入、历史、键映射、补全 + 13 测试
  - `tui/layout_manager.rs`（1475L）— 布局、滚动、格式化 + 48 测试
  - `tui/scene.rs`（224L）— Scene 枚举 + 分类函数 + 12 测试
  - `tui/test_helpers.rs`（121L）— 共享测试辅助函数
- `tui/mod.rs`（148L）— 模块根 + 重导出 + `ReplVisualizationState` + 2 测试
- 总测试数：153（repl: 4 + chat_renderer: 42 + event_renderer: 32 + input_handler: 13 + layout_manager: 48 + scene: 12 + mod: 2）
- `ReplVisualizationState`：19 字段控制所有渲染行为
- `ChatEntry`：13 个变体用于会话内容
- `AgentWorkflowStage`：5 个阶段（Planning/Discovery/Executing/Verifying/Completing）

## Phase 1: repl.rs 提取（3 天）— ✅ Steps 1.1-1.4 完成

### Step 1.1: 创建 `tui/` 模块目录 ✅

- 创建 `crates/interface/src/tui/mod.rs` 作为模块根
- 移动 `chat_renderer.rs` → `tui/chat_renderer.rs`
- 移动 `input_handler.rs` → `tui/input_handler.rs`
- 移动 `layout_manager.rs` → `tui/layout_manager.rs`
- 更新 `repl.rs` 使用 `mod tui;` 替代 `#[path]` 内联模块
- 更新 `lib.rs` 添加 `pub(crate) mod tui;`（如需要）
- **验证**: `cargo test --workspace` — 零行为变更

### Step 1.2: 提取命令路由 + Agent 对话 ✅

- 提取 `handle_command()`, `handle_agent_dialogue()`, `show_agent_error()`, `show_help()` → `tui/commands.rs`
- 提取 `handle_tui_command()`, `restore_session_to_panel()` → `tui/commands.rs`
- 提取 `show_recent_thinking()`, `show_workflow_overview()`, `show_runtime_metrics()`, `show_timeline()`, `show_model_info()`, `show_agent_status()` → `tui/commands.rs`
- **验证**: `cargo check -p ndc-interface` — 零 warning

### Step 1.3: 提取事件渲染 ✅

- 提取 `event_to_lines()` (~320 行) → `tui/event_renderer.rs`
- 提取 `append_recent_thinking()`, `append_recent_timeline()`, `append_workflow_overview()`, `append_token_usage()`, `append_runtime_metrics()` → `tui/event_renderer.rs`
- 提取 `apply_tui_shortcut_action()`, `render_execution_events()` → `tui/event_renderer.rs`
- **验证**: `cargo check -p ndc-interface` — 零 warning

### Step 1.4: 提取 TUI 主循环 ✅

- 提取 `run_repl_tui()` (~510 行) → `tui/app.rs`
- `repl.rs` 仅保留 `ReplConfig`, `ReplState`, `run_repl()` + 测试
- 清理 `repl.rs` 导入（移除所有 ratatui/crossterm 直接依赖，仅保留 `#[cfg(test)]` 条件导入）
- **验证**: `cargo test -p ndc-interface repl::` — 141 测试全通过

### Step 1.5: 移动测试 ✅

- 将 141 个 repl 测试迁移至各模块测试文件（per-module `#[cfg(test)] mod tests`）
- 新增 `tui/test_helpers.rs`（121L）— 共享测试辅助函数（env_lock, with_env_overrides, mk_event, render_event_snapshot, line_plain 等）
- 测试分布：chat_renderer(42) + event_renderer(32) + layout_manager(48) + input_handler(13) + mod.rs(2) + repl(4)
- `repl.rs` 精简至 268 行（含 4 个 repl-specific 测试）
- **验证**: `cargo test -p ndc-interface -- -q` — 全部 153 测试通过

**Phase 1 成果**: `repl.rs` 从 5301 → 268 行。新 `tui/` 模块（9 文件，8181 行含测试）：

```
tui/
├── mod.rs              (~148L)  — 重导出 + ReplVisualizationState + 2 测试
├── app.rs              (~536L)  — TUI 主事件循环（run_repl_tui）
├── chat_renderer.rs    (~1952L) — ChatEntry 模型 + 样式 + DiffPreview + 42 测试
├── commands.rs         (~1084L) — 斜杠命令路由 + 显示 + Agent 对话
├── event_renderer.rs   (~1582L) — 事件渲染 + append 辅助 + 32 测试
├── input_handler.rs    (~791L)  — 输入、历史、键映射、补全 + 13 测试
├── layout_manager.rs   (~1475L) — 布局、滚动、格式化 + 48 测试
├── scene.rs            (~224L)  — Scene 枚举 + 分类（12 测试）
└── test_helpers.rs     (~121L)  — 共享测试辅助函数
```

## Phase 2: Scene 渲染提示（半天，并入 Phase 1 尾部）

> 原计划 2 天，方案 A 精简为 ~0.5 天。不涉及 decision crate。

### Step 2.1: Scene 辅助函数（tui/scene.rs，~50 行）

- 新增 `crates/interface/src/tui/scene.rs`：
  - `Scene` 枚举：`Chat | Analyze | Plan | Implement | Debug | Review`
  - `impl Scene { fn badge_label(&self) -> &str, fn accent_color(&self) -> Color }`
  - `pub(crate) fn classify_scene(workflow_stage: Option<&str>, tool_name: Option<&str>) -> Scene`
  - 映射规则（~30 行 match）：
    - `"planning"` → Plan
    - `"discovery"` → Analyze
    - `"executing"` + write/edit tool → Implement
    - `"executing"` + shell tool → Debug
    - `"verifying"` → Review
    - fallback → Chat
  - 无 LLM 调用 — 纯模式匹配，<1ms
- 在 `tui/mod.rs` 添加 `pub(crate) mod scene;`
- **测试**: 10+ classify_scene 单元测试（按模块内 `#[cfg(test)]`）
- **验证**: `cargo test -p ndc-interface`

### Step 2.2: 接入 event_to_entries()

- 在 `event_to_entries()` 内调用 `classify_scene()` 获取当前 Scene
- 根据 Scene 调整渲染默认值（仅影响展开/折叠/强调色）：
  - Plan: 推理块默认展开
  - Implement: write/edit 工具卡绿色强调
  - Debug: shell 输出橙色强调
  - Review: 验证结果突出
  - Chat/Analyze: 当前行为不变
- 标题栏：在 workflow 进度条后附加场景徽章 `[实现]` 等（复用 build_title_bar 现有逻辑）
- **不新增**: 无 `/scene` 命令、无手动覆盖、无 feature flag、无 config 项
- **测试**: 渲染默认值测试
- **验证**: `cargo test --workspace`

## Phase 3: 渲染增强（2 天）

> 原计划 3 天（含 Scene 感知 + 主题色 + `/scene` 命令 + config），方案 A 精简为 2 天。

### Step 3.1: 内联 Diff 预览（独立于 Scene）

- 当 write/edit 工具完成时（按 `tool_name` 判断，不依赖 Scene）：
  - 解析工具结果获取文件路径和内容变更
  - 生成简单 +/- diff 行（green 添加，red 删除）
  - 插入为新 `ChatEntry::DiffPreview { path, lines, collapsed }` 变体
  - 可通过 Ctrl+D 折叠展开
- 不引入 syntect — 仅 `+` 绿 / `-` 红 / 上下文白色
- **测试**: diff 渲染测试
- **验证**: `cargo test --workspace`

### Step 3.2: 工具类型强调色

- 在 `style_chat_entries()` 中按 `tool_name` 直接应用强调色：
  - write/edit 工具卡：绿色边框
  - shell 工具输出：橙色强调，错误行红色
  - 验证结果：通过绿/失败红
- 仅影响 session 主体区域 — 标题/状态/输入栏不变
- 复用 `TuiTheme` 现有字段，必要时增加 1-2 个色值
- **测试**: 色值应用测试
- **验证**: `cargo test --workspace`

## Phase 4: 收尾（1 天）

> 原计划 2 天（含 feature flag + 集成测试 + 文档），方案 A 精简为 1 天。

### Step 4.1: 集成验证

- `cargo test --workspace` 全通过
- `cargo clippy --workspace --all-features -- -D warnings`
- 终端兼容：80 列 + 256 色终端测试

### Step 4.2: 文档更新

- 更新 TODO.md：标记 P1-Scene 完成
- 更新 current_plan.md
- 更新 USER_GUIDE.md：简要说明自适应渲染行为

## 涉及文件

**将被修改:**

- `crates/interface/src/repl.rs` — 提取为薄层（最终 ~200L）
- `crates/interface/src/chat_renderer.rs` → `tui/chat_renderer.rs`（移动 + 扩展）
- `crates/interface/src/input_handler.rs` → `tui/input_handler.rs`（移动）
- `crates/interface/src/layout_manager.rs` → `tui/layout_manager.rs`（移动 + 扩展）
- `crates/interface/src/lib.rs` — 添加 `tui` 模块

**将被创建:**

- `crates/interface/src/tui/mod.rs` — 模块根 + 重导出
- `crates/interface/src/tui/app.rs` — TuiApp 结构体 + 事件循环
- `crates/interface/src/tui/commands.rs` — 斜杠命令路由
- `crates/interface/src/tui/dialogue.rs` — Agent 对话循环
- `crates/interface/src/tui/event_renderer.rs` — 事件渲染
- `crates/interface/src/tui/scene.rs` — Scene 枚举 + classify_scene()（~50 行）
- `crates/interface/src/tui/tests.rs` — 合并测试

**不涉及:**

- `crates/decision/` — decision crate 不变更，其价值在 P1 核心自治阶段兑现

## 关键决策

- **Scene 是界面层内部细节**（`tui/scene.rs`），不是跨 crate 架构概念
- decision crate 保持独立 — 其 `DecisionEngine` 应在 P1 核心自治阶段接入 orchestrator 做工具执行授权
- 阶段顺序：先提取再添加 Scene — 每个阶段可独立验证
- 不引入 syntect — 简单 +/- diff 着色，后续可增强
- 不新增 `/scene` 命令 — 不暴露 UI 实现细节给用户
- 不新增 feature flag / config 项 — Scene 为渲染优化，无需用户干预
- `AgentWorkflowStage` 保持不变 — Scene 从 stage + tool_name 映射而来
- DiffPreview 按工具类型触发 — 不依赖 Scene 分类
- 旧快捷键 100% 保留 — Scene 添加新行为，不移除任何功能
- 测试跟随代码移动 — 按模块组织测试

## 范围边界

- **包含**: repl.rs 提取、轻量 Scene 渲染提示、工具类型强调色、DiffPreview、标题栏场景徽章
- **排除**: 侧边栏/多面板布局、tui-textarea 集成、`@file` 补全、leader key 系统、动画、syntect 语法高亮、`/scene` 命令、手动场景覆盖、feature flag
- **后续 PR**: leader key / tui-textarea / 动画；decision crate 接入 orchestrator
- **后续阶段**: TUI 独立 Crate 提取 → `docs/design/p1-tui-crate-extraction.md`

## 工期对比

| | 原方案 | 方案 A（精简） | 差异 |
|---|---|---|---|
| Phase 1 | 3 天 | 3 天 | 不变 |
| Phase 2 | 2 天 | 0.5 天 | -1.5 天（Scene 从 decision crate → tui 内部 ~50 行）|
| Phase 3 | 3 天 | 2 天 | -1 天（砍 `/scene` 命令 + config + 手动覆盖）|
| Phase 4 | 2 天 | 1 天 | -1 天（砍 feature flag + 精简文档）|
| **合计** | **10 天** | **6.5 天** | **节省 3.5 天** |
