# P1-TuiCrate: TUI 独立 Crate 提取

> **状态**: ✅ 已完成  
> **前置**: P1-Scene（✅ 已完成）  
> **创建日期**: 2025-07-25  
> **完成日期**: 2025-07-25

---

## 1. 问题分析

P1-Scene 完成后，`crates/interface/src/tui/` 已被重构为 9 个独立子模块、共 ~8181 行代码、153 个测试。
但 TUI 仍作为 `ndc-interface` 的内部模块存在（`pub(crate) mod tui`），带来以下问题：

| 问题 | 影响 |
|------|------|
| TUI 和 CLI/daemon/gRPC 编译耦合 | 修改 TUI 代码触发整个 interface crate 重编译 |
| `pub(crate)` 可见性限制 | TUI 类型无法被其他 crate 复用 |
| 关注点混合 | interface 同时包含交互层（TUI/CLI）和业务逻辑（agent_mode/permission） |
| 依赖传染 | 不需要 TUI 的场景仍被拉入 ratatui/crossterm |

**目标**: 将 TUI 提取为独立 crate `ndc-tui`，实现干净的单向依赖图。

---

## 2. 依赖分析

### 2.1 当前 TUI 对外依赖

| 来源 | 被引用项 | 引用文件 |
|------|---------|---------|
| `crate::redaction` | `RedactionMode`, `sanitize_text` | mod.rs, chat_renderer, event_renderer, commands |
| `crate::agent_mode` | `AgentModeManager` | app.rs, commands.rs |
| `crate::agent_mode` | `PermissionRequest` | app.rs |
| `crate::agent_mode` | `handle_agent_command` | commands.rs |
| `crate::agent_mode` | `AgentModeStatus` | commands.rs |
| `crate::agent_mode` | `ProjectSwitchOutcome`, `AgentProjectCandidate` | commands.rs |
| `ndc_core` | `AgentExecutionEvent`, `AgentExecutionEventKind`, `AgentSessionExecutionEvent`, `AgentWorkflowStage`, `AgentResponse`, `ModelInfo`, `TaskId` | app.rs, event_renderer, commands |
| `crate::repl` | `ReplConfig` | commands.rs |

### 2.2 外部 crate 依赖

| Crate | 版本 | 用途 |
|-------|------|------|
| ratatui | 0.29 | TUI 框架 |
| crossterm | 0.29 | 终端交互 |
| tokio | 1 | 异步运行时 |
| chrono | 0.4 | 时间格式化 |
| tracing | 0.1 | 日志 |

### 2.3 循环依赖风险

**直接提取会产生循环依赖**：

```
ndc-tui → ndc-interface  (需要 agent_mode, redaction)
ndc-interface → ndc-tui  (repl.rs 调用 run_repl_tui)
```

这是本次提取的**核心挑战**，必须通过依赖反转解决。

---

## 3. 架构设计

### 3.1 目标依赖图

```
ndc-core  ←──  ndc-tui  ←──  ndc-interface
   ↑              ↑
   │              │
   └── redaction  │
       (迁入core) │
                  │
          trait AgentBackend
          (定义在 ndc-tui,
           实现在 ndc-interface)
```

**零循环依赖**: `ndc-core ← ndc-tui ← ndc-interface`

### 3.2 Resolution: redaction 迁移至 ndc-core

`redaction.rs`（117 行）仅依赖 `regex` + `std`，是纯工具函数，无业务耦合。

**变更**:
- 将 `crates/interface/src/redaction.rs` 移至 `crates/core/src/redaction.rs`
- 在 `ndc-core` 的 `Cargo.toml` 中添加 `regex = "1"` 依赖
- `ndc-core/src/lib.rs` 新增 `pub mod redaction;`
- 所有引用 `ndc_interface::redaction` 的代码改为 `ndc_core::redaction`

### 3.3 Resolution: AgentBackend trait（依赖反转）

从 TUI 对 `AgentModeManager` 的 12 个方法调用中提取 trait，定义在 `ndc-tui` 中：

```rust
// crates/tui/src/agent_backend.rs

use async_trait::async_trait;
use std::path::PathBuf;
use ndc_core::{AgentExecutionEvent, AgentResponse, ModelInfo, TaskId};

/// TUI 使用的 Agent 交互抽象
#[async_trait]
pub trait AgentBackend: Send + Sync {
    // --- 状态查询 ---
    async fn status(&self) -> AgentStatus;
    async fn session_timeline(&self, limit: Option<usize>)
        -> anyhow::Result<Vec<AgentExecutionEvent>>;
    async fn subscribe_execution_events(&self)
        -> anyhow::Result<(String, tokio::sync::broadcast::Receiver<AgentExecutionEvent>)>;

    // --- 用户输入处理 ---
    async fn process_input(&self, input: &str) -> anyhow::Result<AgentResponse>;

    // --- Provider/Model 切换 ---
    async fn switch_provider(&self, provider: &str, model: Option<&str>)
        -> anyhow::Result<()>;
    async fn switch_model(&self, model: &str) -> anyhow::Result<()>;
    async fn list_models(&self, provider: Option<&str>)
        -> anyhow::Result<Vec<ModelInfo>>;

    // --- Session 管理 ---
    async fn use_session(&self, id: &str, read_only: bool) -> anyhow::Result<String>;
    async fn resume_latest_project_session(&self) -> anyhow::Result<String>;
    async fn start_new_session(&self) -> anyhow::Result<String>;
    async fn list_project_session_ids(&self, prefix: Option<&str>, limit: usize)
        -> anyhow::Result<Vec<String>>;

    // --- 项目上下文 ---
    async fn switch_project_context(&self, path: PathBuf)
        -> anyhow::Result<ProjectSwitchInfo>;
    async fn discover_projects(&self, limit: usize)
        -> anyhow::Result<Vec<ProjectCandidate>>;
}
```

**关键设计决策**:
- `AgentStatus` / `ProjectSwitchInfo` / `ProjectCandidate` 作为简单 DTO 定义在 `ndc-tui` 中（而非使用 interface 的类型）
- `PermissionRequest` 简化为 `ndc-tui` 自定义类型（仅含 description + response channel）
- `handle_agent_command` 内联到 commands.rs 或通过 trait 方法暴露
- ndc-interface 中 `impl AgentBackend for AgentModeManager`

### 3.4 ReplConfig 处理

`ReplConfig` 当前定义在 `crates/interface/src/repl.rs`，被 TUI commands.rs 引用。

**方案**: 将 TUI 需要的配置字段提取为 `TuiConfig`，定义在 `ndc-tui` 中。
`ReplConfig` 保留在 interface，由 repl.rs 构造 `TuiConfig` 传入。

---

## 4. 新 Crate 结构

```
crates/tui/
├── Cargo.toml
└── src/
    ├── lib.rs              # pub mod 声明 + re-exports
    ├── agent_backend.rs    # AgentBackend trait + DTO 类型
    ├── app.rs              # run_repl_tui 主循环
    ├── chat_renderer.rs    # 聊天渲染
    ├── commands.rs         # 命令路由
    ├── event_renderer.rs   # 事件渲染
    ├── input_handler.rs    # 输入处理
    ├── layout_manager.rs   # 布局管理
    ├── scene.rs            # Scene 枚举
    └── test_helpers.rs     # 测试辅助
```

### 4.1 Cargo.toml

```toml
[package]
name = "ndc-tui"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

[dependencies]
ndc-core = { path = "../core" }
ratatui = "0.29"
crossterm = "0.29"
tokio = { version = "1", features = ["full"] }
chrono = "0.4"
tracing = "0.1"
async-trait = "0.1"
anyhow = "1"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
```

### 4.2 可见性变更

所有 `pub(crate)` 项需升级为 `pub`：

| 文件 | 当前可见性 | 目标 |
|------|-----------|------|
| mod.rs — `ReplVisualizationState` | `pub(crate)` | `pub` |
| app.rs — `run_repl_tui` | `pub(crate)` | `pub` |
| commands.rs — 所有函数 | `pub(crate)` | `pub` 或 `pub(crate)` (按需) |
| chat_renderer.rs — `ChatEntry` 等 | `pub(crate)` | `pub` |
| event_renderer.rs — 渲染函数 | `pub(crate)` | `pub` 或内部 |
| input_handler.rs — 处理函数 | `pub(crate)` | `pub` 或内部 |
| layout_manager.rs — 布局函数 | `pub(crate)` | `pub` 或内部 |
| scene.rs — `Scene` | `pub(crate)` | `pub` |

---

## 5. 实施计划

### Phase 1: 前置解耦（2 步）

**Step 1.1**: 迁移 redaction 至 ndc-core
- 移动 `redaction.rs` → `crates/core/src/redaction.rs`
- ndc-core Cargo.toml 添加 `regex = "1"`
- 更新所有引用（interface/grpc/repl → `ndc_core::redaction`）
- 运行 `cargo test --workspace`

**Step 1.2**: 定义 AgentBackend trait
- 在当前 tui/ 中新增 `agent_backend.rs`
- 定义 trait + DTO 类型
- 暂不改变现有代码（仅新增文件）

### Phase 2: Crate 创建与迁移（3 步）

**Step 2.1**: 创建 ndc-tui crate 骨架
- `crates/tui/Cargo.toml`
- `crates/tui/src/lib.rs`
- 工作空间 `Cargo.toml` 添加 member

**Step 2.2**: 移动 TUI 文件
- 将 `crates/interface/src/tui/*.rs` 移至 `crates/tui/src/`
- 更新 `use crate::` → `use crate::` (模块内引用不变)
- 替换 `use crate::redaction::` → `use ndc_core::redaction::`
- 替换 `use crate::agent_mode::` → `use crate::agent_backend::`
- 升级 `pub(crate)` → `pub`

**Step 2.3**: 适配 app.rs / commands.rs
- `AgentModeManager` 参数改为 `Arc<dyn AgentBackend>`
- `PermissionRequest` 改用 ndc-tui 自定义类型
- `AgentModeStatus` 改用 `AgentStatus` (ndc-tui 版本)
- 编译通过

### Phase 3: Interface 适配（2 步）

**Step 3.1**: impl AgentBackend for AgentModeManager
- 在 `ndc-interface` 中实现 trait
- 字段映射 `AgentModeStatus → AgentStatus` 等

**Step 3.2**: 更新 repl.rs
- `use ndc_tui::*` 替换 `use crate::tui::*`
- 构造 `TuiConfig` 并传入
- ndc-interface Cargo.toml 添加 `ndc-tui = { path = "../tui" }`
- interface lib.rs 移除 `pub(crate) mod tui;`

### Phase 4: 验证与清理（2 步）

**Step 4.1**: 全量测试
- `cargo test --workspace` 全部通过
- `cargo clippy --workspace --all-features -- -D warnings` 零警告
- `cargo fmt --all`

**Step 4.2**: 清理与文档
- interface Cargo.toml 移除不需要的 ratatui/crossterm（若无其他引用）
- 更新 CLAUDE.md crate 表格
- 更新相关设计文档

---

## 6. 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| AgentBackend trait 方法签名频繁变动 | 最小化 trait 表面积，仅暴露 TUI 实际调用的方法 |
| DTO 类型冗余（AgentModeStatus vs AgentStatus） | 两者字段相同，impl 中直接 field-by-field 映射 |
| ratatui 版本漂移 | workspace 统一版本管理 |
| 测试依赖 test_helpers 的跨 crate 共享 | test_helpers 保留在 ndc-tui 内部，仅 `#[cfg(test)]` |

---

## 7. 验收标准

- [ ] `crates/tui/` 作为独立 crate 存在于工作空间
- [ ] 依赖图无循环：`ndc-core ← ndc-tui ← ndc-interface`
- [ ] `redaction` 模块位于 `ndc-core` 中
- [ ] `AgentBackend` trait 实现依赖反转
- [ ] 所有 153 个 TUI 测试通过
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace --all-features -- -D warnings` 零警告
- [ ] CLAUDE.md / TODO.md / current_plan.md 已同步更新
