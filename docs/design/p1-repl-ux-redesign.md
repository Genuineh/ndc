# P1-UX：REPL TUI 布局与体验重设计

> 创建时间：2026-02-26  
> 状态：Draft  
> 关联文档：`docs/TODO.md`、`docs/plan/current_plan.md`、`crates/interface/src/repl.rs`

---

## 1. 现状问题分析

### 1.1 布局问题

当前 REPL 采用 4 区垂直布局：

```
┌──────────────────────────────────────────────────────┐ 1行
│ provider=openai model=gpt-4o session=abc workflow=.. │  ← 状态行
├──────────────────────────────────────────────────────┤
│                                                      │
│  NDC - Neo Development Companion                     │  ← Session Body
│  You: 你好                                            │     (可滚动)
│  [Agent] processing...                               │
│  [Tool][r1] start shell ...                          │
│  Assistant:                                          │
│    这是回复                                           │
│                                                      │
├──────────────────────────────────────────────────────┤ 4行
│ Hints:  /help  /provider  /model  ...                │  ← 命令提示
├──────────────────────────────────────────────────────┤ 4行
│ Input (/workflow /tokens ... Esc exit, ↑↓ scroll)    │
│ > _                                                  │  ← 输入区
└──────────────────────────────────────────────────────┘
```

**核心问题：**

| 问题 | 描述 | 影响 |
|------|------|------|
| 状态行过密 | 十余个 `key=value` 挤在 1 行内，信息密度过高 | 用户无法快速定位关键状态 |
| Session Body 视觉扁平 | 用户/助手/工具/推理全部以纯文本 `[Tag]` 前缀渲染，无层次感 | 难以区分对话轮次边界 |
| Hints 区固定占位 | 始终占 4 行，未输入 `/` 时只显示一行引导语 | 浪费宝贵垂直空间 |
| 输入区标题过载 | 窗口标题塞满所有快捷键说明 | 首次有用，之后变为视觉噪声 |
| 缺少 Markdown 渲染 | AI 回复的代码块/列表/标题等全部以纯文本呈现 | 可读性差 |
| 工具调用缺少视觉结构 | 工具 start/done/failed 是独立行，input/output 紧随其后 | 无法一眼看出工具调用边界 |
| 无进度指示器 | 处理中仅显示 `[Agent] processing...` | 用户感知"卡住" |
| 颜色方案硬编码 | 所有颜色散落在 `style_session_log_line()` 中 | 无法主题化、不同终端体验不一致 |

### 1.2 交互问题

| 问题 | 描述 |
|------|------|
| 无输入历史 | 按 ↑/↓ 是滚动而非历史回溯（需 Shift+↑ 或专用键） |
| 无多行输入 | 无法换行输入长 prompt |
| 权限确认混在日志中 | `[Permission]` 行与普通日志混杂，容易错过 |
| 无消息折叠 | 长输出无法折叠，历史消息淹没屏幕 |

### 1.3 参考：OpenCode 设计优势

OpenCode 的 UI 采用 Web 技术（Solid.js），不可直接照搬，但其设计原则值得在 TUI 中对齐：

1. **消息轮次**：每个 User→Assistant 交互作为独立视觉单元（`SessionTurn`）
2. **可折叠工具卡片**：工具调用封装为可展开/折叠的卡片（`BasicTool`）
3. **状态叙事**：处理中显示"正在搜索代码"/"正在编辑"等语义化状态，而非 `processing...`
4. **粘性布局区**：标题始终在上，输入始终在下，中间内容可滚动
5. **智能滚动**：区分"用户主动滚动"和"自动跟随"
6. **权限作为一等公民**：权限请求有专属面板，不与日志混杂

---

## 2. 目标设计

### 2.1 新布局方案

```
┌──────────────────────────────────────────────────────┐
│  NDC   project:myapp   session:abc12   Claude 3.5    │ ← 标题栏 (1行)
├──────────────────────────────────────────────────────┤
│ planning → discovery → [executing] → verifying       │ ← 工作流进度条 (1行)
├──────────────────────────────────────────────────────┤
│                                                      │
│  ╭─ You ─────────────────────────────────────────╮   │
│  │ 请帮我重构 auth 模块                            │   │
│  ╰───────────────────────────────────────────────╯   │
│                                                      │
│  ╭─ Assistant ───────────────────────────────────╮   │ ← 对话区
│  │ 我来分析一下当前的 auth 模块结构：                │   │   (可滚动)
│  │                                                │   │
│  │  ▸ 🔍 read_file auth/mod.rs             200ms │   │ ← 可折叠工具
│  │  ▸ 🔍 grep "pub fn" auth/              120ms │   │
│  │                                                │   │
│  │ 基于分析，我建议按以下方式重构：                   │   │
│  │ 1. 将 JWT 逻辑抽取到 `jwt.rs`                  │   │
│  │ 2. 将中间件移到 `middleware.rs`                  │   │
│  │ ```rust                                       │   │
│  │ pub mod jwt;                                   │   │
│  │ pub mod middleware;                             │   │
│  │ ```                                            │   │
│  ╰───────────────────────────────────────────────╯   │
│                                                      │
├──────────────────────────────────────────────────────┤
│ ⚠ Permission: shell `rm -rf build/` [Y/n/always]    │ ← 权限栏 (按需, 0~2行)
├──────────────────────────────────────────────────────┤
│ ╭ /help │ tokens: 1.2k/session │ ↑History │ Esc ╮   │ ← 状态/提示栏 (1行)
├──────────────────────────────────────────────────────┤
│ > _                                                  │ ← 输入区 (3行)
│                                                      │
╰──────────────────────────────────────────────────────╯
```

### 2.2 区域定义

| 区域 | 高度 | 职责 | 变化 |
|------|------|------|------|
| **标题栏** | 1行 固定 | 项目名、session、模型标识 | 精简为核心信息 |
| **工作流进度** | 1行 固定 | 可视化 workflow 5 阶段进度 | 新增：替代 status 行内 workflow 字段 |
| **对话区** | 弹性 | 消息轮次、工具卡片、AI 回复 | **核心改动**：引入轮次边界、卡片结构 |
| **权限栏** | 0~2行 条件 | 权限确认请求 | 新增：独立区域，按需显示 |
| **状态提示栏** | 1行 固定 | 紧凑状态 + 命令提示 + 快捷键 | 合并原 hints + 部分 status |
| **输入区** | 3行 固定 | 用户输入 + 输入历史 | 缩减标题噪声，增加历史回溯 |

### 2.3 Layout 约束定义

```rust
fn tui_layout_constraints(has_permission: bool) -> Vec<Constraint> {
    let mut c = vec![
        Constraint::Length(1),    // 标题栏
        Constraint::Length(1),    // 工作流进度条
        Constraint::Min(5),       // 对话区 (弹性)
    ];
    if has_permission {
        c.push(Constraint::Length(2));  // 权限栏 (条件)
    }
    c.push(Constraint::Length(1));     // 状态提示栏
    c.push(Constraint::Length(3));     // 输入区
    c
}
```

---

## 3. 详细设计

### 3.1 标题栏

**设计原则**：只显示用户最关心的 3-4 项核心信息。

```
 NDC   project:myapp   session:abc12   claude-3.5-sonnet   idle
```

| 字段 | 说明 | 样式 |
|------|------|------|
| `NDC` | 品牌标识 | Bold + 主题主色 |
| `project:myapp` | 当前项目 | Cyan |
| `session:abc12` | 会话 ID 短码 | DarkGray |
| 模型名 | 当前 LLM 模型 | Green |
| 状态 | `idle` / `thinking...` / `executing...` | Yellow 动态 |

其余详细信息（token 计数、stream 模式、workflow ms 等）移入 `/status` 命令按需查看，或集成到状态提示栏按条件闪现。

### 3.2 工作流进度条

**设计原则**：一目了然当前在哪个阶段，以及整体进度。

```
 planning ─── discovery ─── [executing ◆] ─── verifying ─── completing
```

- 已完成阶段：`Green + Dim`
- 当前阶段：`Cyan + Bold + [方括号]`（附旋转指示器 `◆◇` 或 `⠿⠻⠹⠼` braille spinner）
- 未开始阶段：`DarkGray`
- 空闲时：全部 DarkGray，不占视觉注意力

### 3.3 对话区 —— 消息轮次模型

**核心改动**：从"日志行"模型切换到"消息轮次"模型。

#### 3.3.1 数据模型

```rust
/// 一个完整的对话轮次
struct ChatTurn {
    /// 轮次序号 (r1, r2, ...)
    round: usize,
    /// 用户输入
    user_input: String,
    /// 工具调用列表
    tool_calls: Vec<ToolCallCard>,
    /// 推理/思考（可折叠）
    reasoning: Vec<String>,
    /// 助手回复文本
    assistant_reply: String,
    /// Token 使用量
    token_usage: Option<TokenUsage>,
    /// 时间戳
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// 工具调用卡片
struct ToolCallCard {
    name: String,
    input_summary: String,    // 简短的参数摘要
    output_summary: String,   // 简短的结果摘要
    status: ToolStatus,       // Running / Success / Failed
    duration_ms: u64,
    expanded: bool,           // 是否展开详情
    full_input: String,       // 完整输入 (折叠时隐藏)
    full_output: String,      // 完整输出 (折叠时隐藏)
}
```

#### 3.3.2 用户消息渲染

```
  ╭─ You ─────────────────────────── r3 · 14:32 ╮
  │ 请帮我重构 auth 模块                           │
  ╰──────────────────────────────────────────────╯
```

- 左侧边框：`Blue` 色竖线
- 标题 "You"：`Blue + Bold`
- 轮次号 + 时间：`DarkGray` 右对齐
- 内容：默认前景色

#### 3.3.3 助手回复渲染

```
  ╭─ Assistant ──────────────── r3 · 1.2k tokens ╮
  │                                               │
  │  ▸ read_file auth/mod.rs              ✓ 200ms │  ← 折叠的工具卡片
  │  ▸ grep "pub fn" auth/               ✓ 120ms │
  │  ▾ shell cargo test auth             ✗ 3.2s  │  ← 展开的失败工具
  │    ┊ error[E0412]: cannot find type `Claims`  │
  │    ┊ --> auth/jwt.rs:12:5                     │
  │                                               │
  │  💭 (thinking: 分析依赖关系... 点击展开)         │  ← 折叠的推理
  │                                               │
  │  基于分析，我建议按以下方式重构：                  │
  │  1. 将 JWT 逻辑抽取到 `jwt.rs`                 │
  │  2. 将中间件移到 `middleware.rs`                 │
  │                                               │
  │  ```rust                                      │
  │  pub mod jwt;                                  │
  │  pub mod middleware;                            │
  │  ```                                           │
  ╰────────────────────────────────────────────────╯
```

- 左侧边框：`Cyan` 色竖线
- 工具卡片：`▸`折叠 / `▾`展开，成功 `✓ Green` / 失败 `✗ Red` / 运行中 `⠿ Yellow`
- 推理内容：`Magenta + Dim`，默认折叠
- 助手回复文本：默认前景色
- 代码块：`Gray` 背景 + 语法高亮（简单的关键词着色即可）
- Token 使用量：`DarkGray`，右上角

#### 3.3.4 工具卡片展开

```
  ▾ shell cargo test auth                ✗ 3.2s
    ┊ cmd: cargo test -p auth --lib
    ┊ exit: 1
    ┊ stdout: (23 lines)
    ┊   running 5 tests
    ┊   test jwt::test_parse ... ok
    ┊   test middleware::test_auth ... FAILED
    ┊ stderr: (8 lines)
    ┊   error[E0412]: cannot find type `Claims`
    ┊   --> auth/jwt.rs:12:5
```

- `▾` 指示展开状态
- `┊` 缩进引导线：`DarkGray`
- 标签 `cmd/exit/stdout/stderr`：`Cyan` Bold
- 内容超长时自动截断，显示 `(23 lines)` + 前 N 行

#### 3.3.5 处理中动态状态

处理中的轮次在对话区底部实时更新：

```
  ╭─ Assistant ────────────── r4 · ⠹ thinking... ╮
  │                                               │
  │  ⠿ Searching codebase...                      │  ← 语义化状态
  │    ▸ grep "fn handle_request" src/   running   │
  │                                               │
  ╰────────────────────────────────────────────────╯
```

工具执行时的状态叙事映射（参考 OpenCode）：

| 工具类别 | 叙事文案 |
|----------|----------|
| `read_file` | 📖 Reading file... |
| `grep`/`glob`/`list_dir` | 🔍 Searching codebase... |
| `write_file`/`edit_file` | ✏️ Making edits... |
| `shell` | ⚡ Running command... |
| `ndc_task_*` | 📋 Managing tasks... |
| reasoning | 💭 Thinking... |
| 默认 | ⏳ Working... |

### 3.4 权限栏

**设计原则**：权限请求作为独立视区，不与对话混杂，不可被忽略。

```
 ⚠ Permission Required ──────────────────────────────
   shell: rm -rf build/     risk: High
   [y] Allow  [n] Deny  [a] Always allow  [Esc] Skip
```

- 仅在有待处理权限时显示（`Constraint::Length(2)`），否则不占空间
- 背景色：`Yellow` 边框，醒目
- 快捷键直接在栏内显示

### 3.5 状态提示栏

**设计原则**：合并原 Hints 区与部分 Status 信息为 1 行，上下文敏感。

**默认状态**：
```
 /help │ tokens: 1.2k/4.8k │ stream:live │ ↑↓ scroll │ Tab complete │ Esc exit
```

**输入 `/` 时切换为命令提示**：
```
 /provider  /model  /status  /thinking  /details  /cards  /stream  /clear  ...
```

**选中命令时切换为参数提示**：
```
 /provider: openai  anthropic  minimax  minimax-cn  ollama  openrouter
```

### 3.6 输入区

**改进点**：
- 去掉标题中的大量快捷键文字，仅保留最小标题或无标题
- 支持输入历史（↑/↓），与滚动分离
- 支持多行输入（Shift+Enter 换行，Enter 发送）

```
 ╭───────────────────────────────────────────────╮
 │ > 请帮我重构 auth 模块，需要：_                  │
 │   1. 拆分 JWT 到独立模块                        │
 ╰───────────────────────────────────────────────╯
```

---

## 4. 主题化颜色系统

### 4.1 语义化颜色变量

从硬编码颜色迁移到语义化颜色层，便于后续支持主题切换：

```rust
/// TUI 主题颜色定义
struct TuiTheme {
    // 文本层级
    text_strong: Color,     // 标题、重要文字
    text_base: Color,       // 正文
    text_muted: Color,      // 次要信息
    text_dim: Color,        // 最低层级

    // 语义色
    primary: Color,         // 品牌/主调 (Cyan)
    success: Color,         // 成功 (Green)
    warning: Color,         // 警告 (Yellow)
    danger: Color,          // 错误/危险 (Red)
    info: Color,            // 信息 (Blue)

    // 角色色
    user_accent: Color,     // 用户消息边框 (Blue)
    assistant_accent: Color,// 助手消息边框 (Cyan)
    tool_accent: Color,     // 工具标识 (Gray)
    thinking_accent: Color, // 推理标识 (Magenta)

    // 边框与装饰
    border_normal: Color,   // 普通边框
    border_active: Color,   // 活跃边框
    border_dim: Color,      // 不活跃边框

    // 进度指示
    progress_done: Color,   // 已完成阶段
    progress_active: Color, // 当前阶段
    progress_pending: Color,// 待执行阶段
}

impl TuiTheme {
    fn default_dark() -> Self { /* 暗色主题 */ }
    fn default_light() -> Self { /* 亮色主题 */ }
}
```

### 4.2 预设暗色主题

| 语义 | 暗色终端值 | 用途 |
|------|-----------|------|
| `text_strong` | White | 标题、重点 |
| `text_base` | Gray (248) | 正文 |
| `text_muted` | DarkGray (245) | 次要 |
| `text_dim` | Rgb(100,100,100) | 备注、装饰 |
| `primary` | Cyan | 品牌 |
| `success` | Green | 成功 |
| `warning` | Yellow | 警告 |
| `danger` | Red | 错误 |
| `info` | Blue | 信息 |
| `user_accent` | Blue | You 边框 |
| `assistant_accent` | Cyan | Assistant 边框 |
| `tool_accent` | Gray | 工具 |
| `thinking_accent` | Magenta | 推理 |
| `border_normal` | DarkGray | 边框 |
| `border_active` | Cyan | 焦点 |
| `progress_done` | Green + Dim | 完成 |
| `progress_active` | Cyan + Bold | 当前 |
| `progress_pending` | DarkGray | 未来 |

---

## 5. 交互改进

### 5.1 输入历史

```rust
struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>,  // None = 当前输入, Some(i) = 历史第 i 条
    draft: String,          // 编辑中但未发送的草稿
}
```

- `↑`：上一条历史
- `↓`：下一条历史 / 恢复草稿
- 历史仅在焦点在输入区且未处于滚动模式时激活

### 5.2 多行输入

- `Enter`：发送
- `Shift+Enter` 或 `Alt+Enter`：换行
- 输入区高度可基于内容动态扩展（最大 5 行，超出后滚动）

### 5.3 滚动与焦点分离

| 操作 | 行为 |
|------|------|
| `PageUp/PageDown` | 逐页滚动对话区 |
| `Home/End` | 跳到对话顶部/底部 |
| 鼠标滚轮 | 滚动对话区 |
| `↑/↓` | 输入历史（焦点在输入区时）|
| `Ctrl+↑/Ctrl+↓` | 逐行滚动对话区 |

### 5.4 工具卡片交互

- 使用 `Ctrl+E` 切换全局工具卡片展开/折叠
- 新增：当获得足够上下文后，可考虑数字键快速展开/折叠单个工具卡片

### 5.5 权限快捷响应

当权限栏显示时：
- `y` / `Enter`：允许
- `n`：拒绝  
- `a`：始终允许该类操作
- `Esc`：跳过

---

## 6. 实现路径

### Phase 1：结构改造（基础布局）

**目标**：调整为新布局结构，不改变渲染内容。

1. 将 `tui_layout_constraints` 改为新的 5~6 区动态约束
2. 拆分 `build_status_line` → `build_title_bar` + `build_workflow_progress` + `build_status_hint_bar`
3. 合并 Hints 区与 Status 为 1 行状态提示栏
4. 条件化权限栏区域
5. 输入区去掉标题噪声

**验收**：布局分区正确，现有功能不回退。

### Phase 2：消息轮次模型

**目标**：引入 `ChatTurn` 数据结构，替代纯字符串 `logs: Vec<String>`。

1. 定义 `ChatTurn`、`ToolCallCard` 数据模型
2. 实时事件 → ChatTurn 映射逻辑
3. `ChatTurn` → 带边框 `Line`/`Text` 渲染
4. 用户消息 / 助手回复带视觉边框与轮次标识
5. 工具卡片折叠/展开渲染

**验收**：对话区有清晰轮次边界，工具可折叠。

### Phase 3：样式与主题

**目标**：引入 `TuiTheme`，所有颜色经由主题间接引用。

1. 定义 `TuiTheme` struct 与 `default_dark()` / `default_light()` 后
2. `style_session_log_line()` 全面迁移到使用 theme 引用
3. 工作流进度条加入 spinner 动画
4. 工具状态叙事替代 `[Agent] processing...`

**验收**：颜色一致、可主题化、进度可感知。

### Phase 4：交互增强

**目标**：提升日常使用的流畅度。

1. 输入历史（↑/↓）
2. 多行输入支持（Shift+Enter）
3. 权限区独立交互
4. 焦点管理分离（输入 vs 滚动）
5. 简单 Markdown 渲染（代码块高亮、列表缩进、标题加粗）

**验收**：输入/滚动体验明显改善。

### Phase 5：polish

**目标**：细节打磨。

1. 时间戳格式化
2. Token 使用进度条
3. 长输出截断 + `(N lines, Ctrl+E expand)` 提示
4. 空闲时的优雅等待状态
5. 首次启动引导简化

---

## 7. 设计约束

1. **终端兼容**：必须在 256 色终端下体验良好；真彩色（16M）作为增强而非依赖。
2. **尺寸自适应**：最小 80x24 终端可用；更大终端自动利用额外空间。
3. **性能**：渲染帧率不低于 30fps；5000 行历史下滚动不卡顿。
4. **向后兼容**：所有现有 `/` 命令和 `Ctrl+` 快捷键保持不变。
5. **无额外依赖**：优先使用 ratatui 内置能力；如需 Markdown 渲染可引入轻量 crate（如 `pulldown-cmark`），但须评估编译影响。
6. **TDD**：所有渲染函数可单元测试（输入 → styled Lines 输出）。

---

## 8. 附录：对比总结

| 方面 | 当前 | 目标 |
|------|------|------|
| 布局 | 4 区固定 | 5~6 区动态 |
| 状态行 | 1 行 15+ 字段 | 标题栏(核心) + 进度条 + 状态提示 |
| 对话渲染 | 纯文本行 `[Tag] text` | 轮次边框 + 工具卡片 + Markdown |
| 工具显示 | 行前缀 `[Tool]` | 可折叠卡片 `▸/▾ name status dur` |
| 推理显示 | 行前缀 `[Thinking]` | 折叠区 `💭 (click to expand)` |
| 权限 | 混在日志中 | 独立权限栏 |
| 颜色 | 硬编码 | 语义化主题 |
| 输入 | 单行无历史 | 多行 + 历史回溯 |
| 进度 | `processing...` | 语义化状态 + spinner |
| Hints | 固定 4 行 | 上下文敏感 1 行 |
