# NDC Issues Audit（2026-03-02）

本文记录对「TODO 驱动工作流主流程未触发」与「TUI Session 滚动到底失败」的排查结果。

## 范围

- 核心执行链路：`crates/core/src/ai_agent/conversation_runner.rs`
- TUI 会话滚动：`crates/tui/src/app.rs`、`crates/tui/src/layout_manager.rs`

## 结论总览

| ID | 问题 | 严重级别 | 状态 |
|---|---|---|---|
| CORE-001 | `run_main_loop()` 未接入 `load_context/compress/analysis/planning` 真正执行链路 | High | Fixed |
| CORE-002 | 主流程不会进入 `Reporting` 阶段（只有方法实现与单测，未被主流程调用） | High | Fixed |
| CORE-003 | TODO 驱动方法与主循环脱节（`#[allow(dead_code)]` 方法存在但未落地） | Medium | Fixed |
| TUI-001 | Session 内容过多时滚动条无法到达底部 | High | Fixed |

---

## CORE-001: `run_main_loop()` 未接入前置四阶段

### 修复结果（2026-03-02）

- `run_main_loop()` 已接入 TODO 驱动入口，在生产默认走 8 阶段链路：
  `LoadContext -> Compress -> Analysis -> Planning -> Executing(todo-loop) -> Verifying -> Completing -> Reporting`
- 为避免影响既有测试稳定性：测试环境默认仍可走 legacy 路径；可用环境变量显式切换。
- 新增回归测试覆盖 TODO 驱动主链路的阶段事件与 TodoStateChange 事件发射。

### 现象

实际处理用户输入时，流程主要走旧的 `Planning -> Executing -> Verifying -> Completing` 逻辑，`load_context/compress/analysis/planning` 这套新方法没有在主链路执行。

### 证据

在 `conversation_runner.rs` 中：

- 存在 `load_context()` / `compress_context()` / `run_analysis_round()` / `run_planning_round()` 方法实现与测试；
- 但 `run_main_loop()` 未调用这些方法（方法注释附近也有“will be integrated”的痕迹）。

### 影响

- 新增的上下文快照、压缩、分析、TODO 规划逻辑不会在真实请求中生效；
- 测试通过与线上行为出现偏差（test-only path）。

### 建议修复

1. 在 `run_main_loop()` 中明确串联：`LoadContext -> Compress -> Analysis -> Planning`；
2. 以 `run_main_loop()` 为入口补一条回归测试，断言四阶段事件都被发出；
3. 清理 `#[allow(dead_code)]` 并删掉未接入分支。

---

## CORE-002: 实际不会进入 Reporting

### 修复结果（2026-03-02）

- 主链路已调用 `generate_execution_report()`，并将报告作为最终 `AgentResponse.content` 输出。
- 回归测试验证 execution events 中存在 `Reporting` 阶段。

### 现象

`run_global_verification()` / `run_completion()` / `generate_execution_report()` 已实现并有单测，但真实 `run_main_loop()` 不会调用它们，因此不会产生最终 reporting 结果。

### 证据

- `run_main_loop()` 当前结束路径在 `Completing + session_idle` 收束；
- `generate_execution_report()` 不在主流程调用链。

### 影响

- 用户侧感知为“流程不到 reporting，最终报告缺失”；
- TODO 完成率/变更摘要无法作为主输出统一交付。

### 建议修复

- 在主流程尾部新增 `Reporting` 阶段调用，并将报告作为最终 `AgentResponse.content` 或附加段落；
- 增加回归测试：`run_main_loop()` 的 execution events 至少包含 `Reporting`。

---

## CORE-003: TODO 驱动执行逻辑与主链路脱节

### 修复结果（2026-03-02）

- 主链路已消费 `run_planning_round()` 输出 todos，并逐条执行 `execute_single_todo()`。
- `execute_single_todo()` 现已在真实路径发射 `TodoStateChange`（`in_progress` / `completed`），用于 TUI 侧边栏实时联动。

### 现象

`classify_scenario()` / `run_rounds_with_context()` / `execute_single_todo()` 已具备，但主流程未进入 per-TODO loop。

### 影响

- TDD/Normal/FastPath 分类策略未在真实请求执行；
- TODO 级事件（`TodoExecutionStart/End`）仅在测试路径可见。

### 建议修复

- 让 `run_main_loop()` 消费 `run_planning_round()` 输出的 todos，并逐条调用 `execute_single_todo()`；
- 若暂不切换主链路，至少增加 feature flag（新旧流程可切换）避免半接入状态长期存在。

---

## TUI-001: Session 滚动条无法到达最底部（已修复）

### 根因

滚动与 scrollbar 的范围计算基于“未换行行数”，而实际渲染启用了 `Paragraph::wrap(trim=false)`。

当单行文本被自动换行成多行时：

- 可视总行数 > 计算总行数；
- `max_scroll` 被低估，导致无法滚到真实底部。

### 修复内容

1. 在 `TuiSessionViewState` 增加：`rendered_line_count`；
2. 在 `app.rs` 按 `inner.width` 计算“换行后的可视行数”（`wrapped_visual_line_count`）；
3. 滚动偏移与 scrollbar 范围统一改为使用 `rendered_line_count`。

### 变更文件

- `crates/tui/src/layout_manager.rs`
- `crates/tui/src/app.rs`
- （联动测试结构字段）
  - `crates/tui/src/chat_renderer.rs`
  - `crates/tui/src/input_handler.rs`

### 验证

- `cargo test -p ndc-tui` 通过（170 passed）
- 全量回归通过：
  - `cargo test -p ndc-core -p ndc-tui -p ndc-storage`
  - 结果：291 + 170 + 11 全绿

---

## 备注

本次遵循“先排查和记录、修复明确 UI 缺陷”的策略：

- 核心流程问题已完成根因定位并记录（Open）；
- TUI 滚动到底问题已落地修复（Fixed）。
