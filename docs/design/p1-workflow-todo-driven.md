# P1-Workflow: TODO 驱动工作流重构

> 状态：设计中  
> 前置：P1-TaskTodo ✅  
> 关联：`crates/core/src/ai_agent/mod.rs` · `conversation_runner.rs` · `crates/tui/src/scene.rs`

---

## 1. 问题描述

### 1.1 现状

当前工作流 Pipeline 是 5 阶段线性模型：

```
Planning → Discovery → Executing → Verifying → Completing
```

**问题**:

1. **TODO 不参与工作流编排** — P1-TaskTodo 实现了 TODO 基础设施（创建/展示/持久化），但工作流不知道 TODO 的存在，Agent 不会自动产生或围绕 TODO 执行
2. **无上下文加载阶段** — Agent 直接进入 LLM 对话，不会主动加载工具清单、Skills、MCP 能力、项目记忆等上下文
3. **无上下文压缩** — 上下文超限时无应对策略
4. **无结构化需求分析** — Planning 阶段只是"构建 prompt + context"，不是真正的需求分析
5. **无场景判定** — 编码任务和文档调研任务走相同执行路径，没有 TDD 红绿循环保障
6. **无执行报告** — 完成后无结构化总结（变更清单、测试结果、TODO 完成率）

### 1.2 目标

重构为 **TODO 驱动**的 8 阶段工作流：

- TODO 是工作流的核心编排单元
- 每次用户交互必须产生 TODO（即使只有一项"回答问题"）
- 执行阶段围绕 TODO 逐项进行，区分编码/普通场景
- 编码场景强制 TDD 红绿循环
- 工作流可观测（每阶段有明确的 TUI 反馈和进度）

---

## 2. 新 Pipeline 设计

### 2.1 8 阶段总览

```
┌─────────────────────────────────────────────────────────────────┐
│                     NDC TODO-Driven Workflow                     │
├─────────┬───────────┬──────────┬──────────┬─────────────────────┤
│ ①       │ ②         │ ③        │ ④        │ ⑤                   │
│ Load    │ Compress  │ Analysis │ Planning │ Executing           │
│ Context │ (可跳过)  │          │ →TODO    │ (Per-TODO Loop)     │
├─────────┴───────────┴──────────┴──────────┼─────────────────────┤
│ ⑥ Verifying  │ ⑦ Completing  │ ⑧ Reporting                     │
└──────────────┴───────────────┴──────────────────────────────────┘
```

### 2.2 阶段详细定义

#### Stage 1: LoadContext（加载上下文）

**职责**: 在 LLM 对话之前，主动收集执行所需的上下文信息。

**加载内容**:
- **工具清单**: 已注册的 Tool schemas（名称 + 描述 + 参数签名）
- **Skills**: 已注册的 Skill 定义（如果有）
- **MCP 能力**: 已连接的 MCP server 提供的工具/资源
- **项目记忆**: 从 Storage 查询当前 project 的 GoldMemory facts
- **会话历史**: 当前 session 的对话历史（已有）
- **CLAUDE.md / 项目约束**: 项目级指令文件

**实现要点**:
- `conversation_runner.rs` 新增 `load_context()` 方法
- 收集结果写入 `ContextSnapshot` 结构体
- 通过 `emit_workflow_stage(LoadContext, detail)` 通知 TUI
- detail 内容示例: `"tools=23, skills=2, mcp=1, memories=15"`

**与现状差异**: 当前 prompt 构建散落在 `build_messages()` 和 `build_working_memory_injector()` 中，没有独立阶段。新设计将这些收集动作统一到 LoadContext 阶段，使其可观测。

#### Stage 2: Compress（上下文压缩）

**职责**: 当总上下文 token 超过模型窗口阈值时，执行压缩策略。

**触发条件**: `context_tokens > model_max_tokens * compression_threshold`（默认 threshold=0.7）

**压缩策略**:
- **优先裁剪**: 早期对话轮次（保留最近 N 轮）— 已有 `truncate_messages()` 实现
- **摘要替换**: 将被裁剪的对话替换为 LLM 生成的摘要
- **工具结果压缩**: 大型工具输出只保留关键信息
- **可跳过**: 上下文未超限时直接进入 Analysis，detail="skipped"

**实现要点**:
- 复用已有 `truncate_messages()` 作为基础
- 新增 token 估算函数 `estimate_context_tokens(messages) -> usize`
- 压缩后仍超限则 warn 并继续（best-effort）

#### Stage 3: Analysis（需求分析）

**职责**: LLM 结合完整上下文，对用户需求进行结构化分析。

**分析输出**（注入到对话中）:
- **需求理解**: 用户诉求的一句话总结
- **影响范围**: 涉及的文件/模块/接口
- **约束识别**: 从 CLAUDE.md、项目配置中提取的硬性约束
- **场景预判**: 是编码任务、文档任务、调研任务还是混合任务
- **风险点**: 可能的破坏性变更、依赖冲突

**实现要点**:
- 这是一次独立的 LLM 调用（analysis prompt）
- 结果以 JSON 结构返回
- 解析后存入 `AnalysisResult` 结构体
- `AnalysisResult` 作为下一阶段 Planning 的输入

#### Stage 4: Planning（规划 → 产生 TODO）

**职责**: 基于 Analysis 结果，将任务分解为有序 TODO 列表。**这是强制阶段 — 必须产生至少 1 个 TODO**。

**Planning 输出**:
- TODO 列表：`Vec<PlanItem>` — 每项含标题、描述、场景类型（coding/normal）、预期验证方式
- 依赖关系：线性顺序（当前暂不支持 DAG 并行）

**场景分类规则**:
```rust
pub enum TodoExecutionScenario {
    /// 涉及代码文件变更 → 强制 TDD 红绿循环 (包含降级机制)
    Coding,
    /// 配置、文档、调研等 → 普通执行
    Normal,
    /// 单步短命令或简单问答 → 快路径执行，跳过计划及全局检查
    FastPath,
}
```

- 涉及 `.rs` / `.ts` / `.py` 等源码文件修改 → `Coding`
- 涉及 `.md` / `.yaml` / `.toml` 配置或纯对话 → `Normal`
- 不确定时默认 `Coding`（偏向安全侧）

**实现要点**:
- 这是一次独立的 LLM 调用（planning prompt），输入包含 `AnalysisResult`
- 输出 JSON array，每项映射为 `Task::new_todo()` 写入 Storage
- 空列表时自动补一条"回答用户问题"兜底 TODO
- TUI 在此阶段结束后刷新 TODO sidebar

#### Stage 5: Executing（执行循环 — Per-TODO）

**职责**: 遍历 TODO 列表，逐项执行。这是工作流的主体阶段。

**Per-TODO 执行流程**:

```
for each todo in todo_list (按顺序):
    1. mark_in_progress(todo)
    2. classify_scenario(todo) → Coding / Normal

    if Coding:
        a. Red:    LLM 生成失败测试（使用 write/edit 工具）
        b. Green:  LLM 编写最小实现使测试通过
        c. Regress: 执行 shell 工具运行 `cargo test` / 测试命令，确认不破
        d. Doc:    更新相关文档（如需）

    if Normal:
        a. Execute: LLM 执行任务（可能多轮工具调用）
        b. Verify:  验证结果（shell 检查 / 文件内容确认）
        c. Doc:    更新相关文档（如需）

    3. review: LLM 自审单项结果，判定 pass/fail
    4. if pass: mark_completed(todo)
       if fail: mark_failed(todo), 记录原因, 可选重试
```

**与现有 Executing 的关系**:
- 现有的 LLM round loop（`round 1..N`）被保留，但嵌套在 per-TODO 循环内
- 每个 TODO 的执行可能跨越多个 LLM rounds
- System prompt 注入当前 TODO 上下文: "你正在执行 TODO #3: xxx，场景: Coding/TDD"

**TDD 路径的 system prompt 补充**:
```
当前任务为编码场景，必须遵循 TDD 红绿循环：
1. 先写一个会失败的测试（Red），使用 write_file 工具创建测试代码
2. 运行测试确认失败（使用 shell 工具: cargo test）
3. 编写最小实现使测试通过（Green）
4. 运行全部测试确认无回归
5. 有需要则更新文档
严禁跳过测试步骤。
```

**Progress 反馈**:
- Workflow Progress Bar 显示: `Executing TODO 3/7 — "编写认证测试"`
- TODO sidebar 实时更新当前项状态图标
- 每个 TODO 完成后 emit `TodoCompleted` 事件

#### Stage 6: Verifying（全局验证）

**职责**: 所有 TODO 执行完成后，进行一次全局回归验证。

**验证内容**:
- 运行完整测试套件（`cargo test --workspace` 或项目测试命令）
- 检查是否有被遗忘的 TODO（仍为 Pending/InProgress 状态的）
- 检查变更文件列表是否与 TODO 范围一致
- 检查是否有未提交的编译错误

**失败处理**:
- 全局测试失败 → 标记具体 TODO 为 Failed，回到 Executing 阶段进行修复
- 遗漏 TODO → 提醒用户确认是否放弃

#### Stage 7: Completing（完成收尾）

**职责**: 文档收尾和知识回灌。

- 更新 CHANGELOG / 设计文档（如需）
- 将关键发现写入 GoldMemory（经验回灌）
- 清理临时文件
- 与现有 Completing 阶段合并

#### Stage 8: Reporting（执行报告）

**职责**: 生成结构化的执行报告，作为 Agent 最终回复。

**报告内容**:
```
## 执行报告

### 完成情况
- TODO 完成率: 7/7 (100%)
- 总轮次: 23 rounds
- 总耗时: 3m 42s

### 变更摘要
- 新增文件: 2 (crates/tui/src/todo_panel.rs, ...)
- 修改文件: 5
- 新增测试: 12

### 测试结果
- 全量测试: 167 passed, 0 failed
- 回归验证: ✅ 通过

### TODO 明细
1. ✅ 创建 TODO 面板渲染组件
2. ✅ 添加 Ctrl+O 快捷键
...
```

---

## 3. Core 模型变更

### 3.1 `AgentWorkflowStage` 扩展

```rust
// crates/core/src/ai_agent/mod.rs

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkflowStage {
    LoadContext,   // 1 — 加载上下文
    Compress,      // 2 — 上下文压缩（可跳过）
    Analysis,      // 3 — 需求分析
    Planning,      // 4 — 规划产生 TODO
    Executing,     // 5 — Per-TODO 执行循环
    Verifying,     // 6 — 全局验证
    Completing,    // 7 — 文档收尾
    Reporting,     // 8 — 生成报告
}

impl AgentWorkflowStage {
    pub const TOTAL_STAGES: u32 = 8;

    pub fn index(self) -> u32 {
        match self {
            Self::LoadContext => 1,
            Self::Compress => 2,
            Self::Analysis => 3,
            Self::Planning => 4,
            Self::Executing => 5,
            Self::Verifying => 6,
            Self::Completing => 7,
            Self::Reporting => 8,
        }
    }
}
```

### 3.2 新增执行场景枚举

```rust
// crates/core/src/ai_agent/mod.rs

/// TODO 执行场景分类
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoExecutionScenario {
    /// 代码变更场景 — 强制 TDD 红绿循环 (支持 Scene 降级)
    Coding,
    /// 非代码场景 — 执行→验证→文档
    Normal,
    /// 单步短命令或纯文答 — 快路径跳过多余步骤
    FastPath,
}
```

### 3.3 新增事件类型

```rust
// AgentExecutionEventKind 扩展
pub enum AgentExecutionEventKind {
    // ... 现有 ...
    /// TODO 状态变更事件（用于 TUI 实时刷新）
    TodoStateChange,
    /// 分析结果事件
    AnalysisComplete,
    /// 规划结果事件（含 TODO 列表）
    PlanningComplete,
    /// 单项 TODO 执行开始
    TodoExecutionStart,
    /// 单项 TODO 执行结束
    TodoExecutionEnd,
    /// 执行报告
    Report,
}
```

### 3.4 上下文快照

```rust
// crates/core/src/ai_agent/mod.rs

/// LoadContext 阶段的输出
#[derive(Debug, Clone)]
pub struct ContextSnapshot {
    pub tool_count: usize,
    pub skill_count: usize,
    pub mcp_tool_count: usize,
    pub memory_facts: Vec<String>,
    pub project_constraints: Vec<String>,
    pub estimated_tokens: usize,
}

/// Analysis 阶段的输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub summary: String,
    pub affected_scope: Vec<String>,
    pub constraints: Vec<String>,
    pub scenario_hint: TodoExecutionScenario,
    pub risks: Vec<String>,
}
```

---

## 4. ConversationRunner 重构

### 4.1 `run_main_loop()` 新流程

```rust
pub async fn run_main_loop(&mut self, ...) -> Result<AgentResponse, AgentError> {
    // Stage 1: LoadContext
    self.emit_workflow_stage(LoadContext, "start").await;
    let ctx_snapshot = self.load_context().await?;
    self.emit_workflow_stage(LoadContext,
        &format!("tools={}, memories={}", ctx_snapshot.tool_count, ctx_snapshot.memory_facts.len())
    ).await;

    // Stage 2: Compress (conditional)
    self.emit_workflow_stage(Compress, "evaluating").await;
    let messages = if ctx_snapshot.estimated_tokens > self.token_threshold() {
        self.compress_context(messages, &ctx_snapshot).await?
    } else {
        self.emit_workflow_stage(Compress, "skipped").await;
        messages
    };

    // Stage 3: Analysis
    self.emit_workflow_stage(Analysis, "analyzing_requirements").await;
    let analysis = self.run_analysis_round(&messages).await?;
    // analysis result 注入到对话

    // Stage 4: Planning (必须产生 TODO)
    self.emit_workflow_stage(Planning, "generating_todo_list").await;
    let todo_list = self.run_planning_round(&analysis).await?;
    // todo_list 写入 Storage, emit PlanningComplete 事件

    // Stage 5: Executing (Per-TODO 循环)
    for (idx, todo) in todo_list.iter().enumerate() {
        self.emit_workflow_stage(Executing,
            &format!("todo_{}_of_{}: {}", idx+1, todo_list.len(), todo.title)
        ).await;
        self.execute_single_todo(todo, &analysis).await?;
    }

    // Stage 6: Verifying
    self.emit_workflow_stage(Verifying, "global_regression").await;
    let verification = self.run_global_verification().await?;

    // Stage 7: Completing
    self.emit_workflow_stage(Completing, "documentation_and_memory").await;
    self.run_completion(&todo_list).await?;

    // Stage 8: Reporting
    self.emit_workflow_stage(Reporting, "generating_report").await;
    let report = self.generate_execution_report(&todo_list, &verification).await?;

    Ok(AgentResponse { content: report, ... })
}
```

### 4.2 `execute_single_todo()` — 单 TODO 执行

```rust
async fn execute_single_todo(&mut self, todo: &TodoItem, analysis: &AnalysisResult) {
    self.backend.update_todo_state(todo.index, TodoState::InProgress).await?;
    self.emit_event(TodoExecutionStart, todo.title).await;

    let scenario = self.classify_scenario(todo, analysis);

    match scenario {
        TodoExecutionScenario::Coding => {
            self.execute_tdd_cycle(todo).await?;
        }
        TodoExecutionScenario::Normal => {
            self.execute_normal_cycle(todo).await?;
        }
    }

    // Per-TODO review
    let review_passed = self.review_todo_result(todo).await?;

    if review_passed {
        self.backend.complete_todo(todo.index).await?;
    } else {
        self.backend.update_todo_state(todo.index, TodoState::Failed).await?;
    }

    self.emit_event(TodoExecutionEnd, todo.title).await;
}
```

### 4.3 `execute_tdd_cycle()` — TDD 红绿循环

```rust
async fn execute_tdd_cycle(&mut self, todo: &TodoItem) {
    // TDD system prompt injection
    let tdd_prompt = format!(
        "当前执行 TODO #{}: {}\n\
         场景: 编码(Coding) — 必须遵循 TDD 红绿循环:\n\
         1. Red:   先写一个会失败的测试\n\
         2. Green: 写最小实现使测试通过\n\
         3. Regress: 运行全部测试确认无回归\n\
         4. Doc:   更新相关文档（如需）\n\
         严禁跳过测试步骤。",
        todo.index, todo.title
    );

    // 注入 TDD prompt 后进入 LLM round loop
    // round loop 保持现有逻辑（多轮工具调用）
    self.run_rounds_with_context(&tdd_prompt).await?;
}
```

---

## 5. 高阶优化与边界处理（机制增强）

### 5.1 门禁与 Review 强化机制 (Custom Skills & Hard Hooks)
虽然保持现有的 LLM 自审（Review）流程，但为了克服其不可靠性，引入项目级门禁机制：
- **门禁技能 (Review Skills)**：支持从 `<project_root>/.ndc/skills/` 自动加载专用的审查 skill（例如团队特有的代码规范要求）。Review 阶段会将其挂载到上下文中辅助审查。
- **硬性钩子 (Hard Hooks)**：支持使用脚本进行硬性阻断。在当前项目目录 `<project_root>/.ndc/hooks/` 下添加门禁脚本（如 `pre-review.sh`）。LLM 单项 Review 决断前，必须先执行 hook 脚本检测，若退出码不为 0（如 `cargo test` 或是 `clippy` 报错），则强制驳回（判定为 Fail）并拦截状态流转，要求模型直接进入修复循环。

### 5.2 多级折叠与上下文压缩 (Multi-level Rolling Context)
针对多次执行后产生的上下文雪崩问题：
- **多级折叠**：当前 TODO 执行循环完成（状态变为 Completed）后，不再将长篇的工具调用与环境输出（ToolCall -> ToolResult）历史保留在主窗口内。LLM 将该 TODO 的执行过程总结为短摘要。
- **结合记忆系统**：把总结内容压缩并写回 NDC 原生的 `WorkingMemory` / `SessionMemory` 系统。当前会话后续的 TODO 将抛弃沉重的流水记录，只需加载这些结构化记忆，从而极大程度节约 Token 消耗，保持核心思路清晰。

### 5.3 基于 Scene 概念的 TDD 死锁降级
编码（Coding）场景中强制 TDD 可能导致 LLM 纠结于写不出合适的报错用例而死循环：
- **复用既有 Scene 概念**：深挖已存在的 `Scene`（Plan / Implement / Debug / Review）模型。执行单个 TODO 时，如果由于测试不通过导致连续停留在 `Debug` 场景（Red 环节）循环纠缠（例如超过 2 轮），则触发**场景降级**。
- **降级路径**：跳过严格的失败测试用例编写，使整体场景回退为更宽松的 `Implement`（直写实现代码），然后只依靠外层最终的 `Verifying`（全局回归）兜底，打破局部 TDD 导致的重试死锁。

### 5.4 快速路径与状态重入 (Fast-Path & Resumability)
- **短路快路径**：对于简单提问或单一短命令，在 `Analysis` 阶段直接标记出 `scenario_hint: FastPath`。对应生成隐式任务并**短路跳过** `Planning` 环节（直接挂靠），执行完成也无需进入最后的全局 `Verifying` 扫描，降低无意义的环节拖延感。
- **状态持久化重入**：依赖于 `Task` 的 SQLite 持久化能力。如果运行时发生系统中断（Ctrl+C 或断网），重新启动切入到同一 Session 时，`LoadContext` 若探查到数据库中有处于 `Pending` / `InProgress` 的列表，将主动打断常规的分析，提示直接接续恢复断点（Resume），跳回 Stage 5 继续先前的 Execute 进度。

---

## 6. TUI 适配

### 6.1 Scene 映射更新

```rust
// crates/tui/src/scene.rs

pub fn classify_scene(workflow_stage: Option<&str>, tool_name: Option<&str>) -> Scene {
    match workflow_stage {
        Some("load_context") => Scene::Analyze,    // 加载上下文 → 分析色
        Some("compress") => Scene::Analyze,        // 压缩 → 分析色
        Some("analysis") => Scene::Analyze,        // 需求分析 → 分析色
        Some("planning") => Scene::Plan,           // 规划 → 规划色
        Some("executing") => match tool_name {     // 执行 → 按工具细分
            Some(t) if is_write_tool(t) => Scene::Implement,
            Some(t) if is_shell_tool(t) => Scene::Debug,
            _ => Scene::Implement,
        },
        Some("verifying") => Scene::Review,        // 验证 → 验证色
        Some("completing") => Scene::Review,       // 收尾 → 验证色
        Some("reporting") => Scene::Review,        // 报告 → 验证色
        _ => Scene::Chat,
    }
}
```

### 6.2 Workflow Progress Bar 更新

进度条从 5 阶段改为 8 阶段，百分比计算:

```
LoadContext: 12.5% (1/8)
Compress:   25.0% (2/8)
Analysis:   37.5% (3/8)
Planning:   50.0% (4/8)
Executing:  62.5% (5/8) — detail 显示 "TODO 3/7: xxx"
Verifying:  75.0% (6/8)
Completing: 87.5% (7/8)
Reporting: 100.0% (8/8)
```

Executing 阶段内部，progress bar detail 格式: `"62%(5/8) TODO 3/7: 编写认证测试"`

### 6.3 TODO Sidebar 实时联动

- `TodoStateChange` 事件驱动 sidebar 即时刷新（不等 Agent 回合结束）
- 当前执行的 TODO 显示 `◎` 图标 + 黄色高亮
- Executing 阶段 progress detail 包含当前 TODO 标题

---

## 7. 实施分 Phase 细节

### Phase 1: Core 模型扩展

**变更文件**:
- `crates/core/src/ai_agent/mod.rs`:
  - `AgentWorkflowStage` 枚举 5→8 变体
  - `TOTAL_STAGES` 5→8
  - `as_str()` / `parse()` / `index()` 更新
  - 新增 `TodoExecutionScenario` 枚举
  - 新增 `ContextSnapshot` / `AnalysisResult` 结构体
  - `AgentExecutionEventKind` 新增 6 个变体
- `crates/tui/src/scene.rs`:
  - `classify_scene()` 扩展 3 个新阶段映射
- `crates/tui/src/layout_manager.rs`:
  - `workflow_progress_descriptor()` 适配 8 阶段

**测试（Red→Green）**:
- `AgentWorkflowStage` 8 变体的 `as_str` / `parse` / `index` 往返
- `TodoExecutionScenario` 序列化/反序列化
- `classify_scene` 新阶段映射
- `TOTAL_STAGES == 8`
- 进度百分比计算（8 阶段均分）

### Phase 2: ConversationRunner 前 4 阶段

**变更文件**:
- `crates/core/src/ai_agent/conversation_runner.rs`:
  - `run_main_loop()` 结构重构
  - 新增 `load_context()` 方法
  - 新增 `compress_context()` 方法（复用 `truncate_messages()`）
  - 新增 `run_analysis_round()` 方法（独立 LLM 调用）
  - 新增 `run_planning_round()` 方法（独立 LLM 调用 → TODO 创建）
  - 新增 `estimate_context_tokens()` 辅助函数

**测试（Red→Green）**:
- `load_context()` 返回正确的工具/记忆数量
- `compress_context()` 超限时裁剪、未超限跳过
- `estimate_context_tokens()` 估算精度（±20%）
- `run_planning_round()` 必须产生至少 1 个 TODO
- 空 planning 输出时自动补兜底 TODO

### Phase 3: TODO 执行循环

**变更文件**:
- `crates/core/src/ai_agent/conversation_runner.rs`:
  - 新增 `execute_single_todo()` 方法
  - 新增 `classify_scenario()` 方法
  - 新增 `execute_tdd_cycle()` 方法
  - 新增 `execute_normal_cycle()` 方法
  - 新增 `review_todo_result()` 方法
  - 现有 round loop 封装为 `run_rounds_with_context()`

**测试（Red→Green）**:
- `classify_scenario()` 按文件类型正确分类
- TDD cycle 注入 TDD prompt
- Normal cycle 不注入 TDD prompt
- TODO 状态自动流转: Pending → InProgress → Completed/Failed
- 失败 TODO 记录原因

### Phase 4: Verifying + Completing + Reporting

**变更文件**:
- `crates/core/src/ai_agent/conversation_runner.rs`:
  - 新增 `run_global_verification()` 方法
  - 新增 `run_completion()` 方法
  - 新增 `generate_execution_report()` 方法

**测试（Red→Green）**:
- 全局验证检测遗漏 TODO
- 全局验证运行测试命令
- 报告包含完成率/变更摘要/测试结果
- 全部 TODO 完成时报告标记 100%

### Phase 5: TUI 适配

**变更文件**:
- `crates/tui/src/scene.rs`: `classify_scene()` 更新
- `crates/tui/src/layout_manager.rs`: progress bar 8 阶段
- `crates/tui/src/event_renderer.rs`: 新事件类型渲染
- `crates/tui/src/app.rs`: `TodoStateChange` 事件驱动 sidebar 刷新

**测试（Red→Green）**:
- 新 Scene 映射测试（load_context/compress/analysis/reporting → 正确 Scene）
- Progress bar 8 阶段百分比
- TODO 执行期间 progress detail 格式
- `TodoStateChange` 事件触发 sidebar 刷新

### Phase 6: 端到端测试 + 文档

**验证项**:
- 完整流程: 输入 → LoadContext → Compress → Analysis → Planning → Executing(TDD) → Verifying → Completing → Reporting
- TODO 自动生成 + 持久化 + 恢复
- 编码场景 TDD 路径实际执行
- 普通场景快速执行
- 报告结构验证

**文档更新**:
- `docs/USER_GUIDE.md` — 更新工作流阶段说明
- `docs/TODO.md` — P1-Workflow 状态更新
- `CLAUDE.md` — 如需更新

---

## 8. 兼容性与迁移

### 8.1 向后兼容

- `AgentWorkflowStage::Planning` 保留但语义变更: 旧 Planning = build_prompt，新 Planning = 产生 TODO
- 旧的 `Discovery` 阶段语义并入 `Analysis`（工具发现作为 LoadContext 的一部分）
- 序列化格式（数据库中已有的 timeline 事件）: `serde(rename_all = "snake_case")` 确保新旧值共存

### 8.2 渐进迁移策略

- Phase 1-2 完成后可先部署（前 4 阶段），Executing 仍走旧逻辑（round loop without per-TODO）
- Phase 3 加入 per-TODO 循环后完整切换
- 通过环境变量 `NDC_WORKFLOW_VERSION=v1|v2` 可短期共存（可选，按需）

---

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| Analysis/Planning 额外 LLM 调用增加延迟 | 用户等待时间增加 | 简单任务合并 Analysis+Planning 为一次调用 |
| TDD 路径增加 round 数 | token 消耗增加 | 编码场景才走 TDD，普通场景快路径 |
| Planning 产生不合理 TODO | 执行偏离 | TODO 产生后先展示给用户确认（可选 `/plan confirm`） |
| 现有测试大量需要适配 | 重构阻力 | Phase 1 先扩展枚举（追加不删除），不改已有测试 |
| Executing 阶段 per-TODO 循环复杂度 | 调试困难 | 每 TODO 执行有独立 timeline 事件链，可单独回溯 |
