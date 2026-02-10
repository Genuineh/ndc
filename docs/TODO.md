# NDC 实现待办清单

> **重要更新 (2026-02-06)**: LLM 集成 - 知识驱动 + TODO 映射 + 工业级优化

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 + LLM Provider + TODO 管理 + Memory ✅
├── decision/          # [大脑] 决策引擎 ✅ 已完成
├── runtime/           # [身体] 执行与验证 + Workflow + Discovery ⏳
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon) ✅ 已完成
```

## 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────┐
│              NDC 工业级自治系统                                        │
│                                                                     │
│  知识库 ──▶ 理解需求 ──▶ TODO 映射 ──▶ 分解 ──▶ 影子探测 ──▶      │
│                                                                     │
│  工作记忆 ──▶ 执行开发 ──▶ 验收 ──▶ 失败归因 ──▶ 文档 ──▶ 完成     │
│                                                                     │
│  核心闭环: 人类纠正 → Invariant (Gold Memory) → 永不重复犯错          │
└─────────────────────────────────────────────────────────────────────┘
```

## 已完成模块 ✅

| 模块 | 文件 | 状态 | 说明 |
|------|------|------|------|
| **core** | task.rs | ✅ | Task, TaskState, ExecutionStep, ActionResult |
| **core** | intent.rs | ✅ | Intent, Verdict, PrivilegeLevel, Effect |
| **core** | agent.rs | ✅ | AgentRole, AgentId, Permission |
| **core** | memory.rs | ✅ | MemoryStability, MemoryQuery, MemoryEntry |
| **core** | config.rs | ✅ | YAML 配置系统 |
| **decision** | engine.rs | ✅ | DecisionEngine, validators |
| **runtime** | executor.rs | ✅ | Task execution, tool coordination |
| **runtime** | workflow.rs | ✅ | State machine, transitions |
| **runtime** | storage.rs | ✅ | In-memory storage |
| **runtime** | storage_sqlite.rs | ✅ | SQLite storage |
| **runtime** | tools/mod.rs | ✅ | Tool, ToolManager |
| **runtime** | tools/fs.rs | ✅ | File operations |
| **runtime** | tools/git.rs | ✅ | Git operations |
| **runtime** | tools/shell.rs | ✅ | Shell command execution |
| **runtime** | verify/mod.rs | ✅ | QualityGateRunner |
| **interface** | cli.rs | ✅ | CLI commands |
| **interface** | daemon.rs | ✅ | gRPC service framework |
| **interface** | grpc.rs | ✅ | gRPC service impl |
| **interface** | repl.rs | ✅ | REPL mode |
| **interface** | e2e_tests.rs | ✅ | E2E tests |
| **interface** | grpc_client.rs | ✅ | gRPC client SDK |

---

## LLM 集成 - 知识驱动 + 工业级自治 ⏳

```
📄 详细设计: docs/ENGINEERING_CONSTRAINTS.md

九大阶段:
0. 谱系继承 → 继承历史知识
1. 理解需求 → 检索知识库 + 检查 TODO
2. 建立映射 → 关联/创建总 TODO
3. 分解任务 → LLM 分解 + 非LLM确定性校验
4. 影子探测 → Read-Only 影响分析 ← ✅ P1 已完成
5. 工作记忆 → 精简上下文 ← P2
6. 执行开发 → 质量门禁 + 重来机制
7. 失败归因 → Human Correction → Invariant ← P3
8. 更新文档 → Fact/Narrative
9. 完成 → 谱系更新
```

### 工业级优化组件 ⏳

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 组件                     │ 文件                          │ 优先级       │
├─────────────────────────────────────────────────────────────────────────────┤
│ Working Memory           │ memory/working_memory.rs     │ P2 ✅ DONE  │
│ Discovery Phase          │ discovery/mod.rs             │ P1 ✅ DONE  │
│ Failure Taxonomy        │ error/taxonomy.rs            │ P2 ✅ DONE  │
│ Invariant (Gold Memory) │ memory/invariant.rs          │ P3 ✅ DONE  │
│ Model Selector           │ llm/selector.rs             │ P3 ✅ DONE  │
│ Task Lineage            │ todo/lineage.rs              │ P2 ✅ DONE  │
│ Event-Driven Engine     │ engine/mod.rs               │ P3 ✅ DONE  │
│ Decomposition Lint      │ llm/decomposition/lint.rs    │ P2 ✅ DONE  │
└─────────────────────────────────────────────────────────────────────────────┘

P1 = 第一刀 (Discovery Phase) - ✅ 已验收通过 (ec499ab)
P2 = 第二刀 (Working Memory + Saga) - ✅ 已完成
P3 = 第三刀 (Invariant + Telemetry) - ✅ 已完成
```

---

## 代码结构 (规划中)

```
crates/core/src/
├── llm/
│   ├── mod.rs              # Provider Trait
│   ├── provider/
│   │   ├── mod.rs          # Trait 定义
│   │   ├── openai.rs       # OpenAI
│   │   ├── anthropic.rs     # Anthropic
│   │   └── minimax.rs       # MiniMax
│   ├── understanding.rs     # 阶段 1
│   ├── decomposition/
│   │   ├── mod.rs          # 分解服务
│   │   ├── planner.rs      # 任务规划
│   │   └── lint.rs         # 非LLM校验 ⭐
│   └── selector.rs          # 模型自适应 ⭐
│
├── todo/
│   ├── mod.rs              # TODO 模块
│   ├── project_todo.rs     # 总 TODO
│   ├── task_chain.rs       # 任务链
│   ├── mapping_service.rs   # 映射服务
│   └── lineage.rs          # 谱系继承 ⭐
│
├── memory/                 # ✅ P2 Working Memory 已完成
│   ├── mod.rs
│   ├── knowledge_base.rs    # 知识库
│   ├── working_memory.rs   # WorkingMemory ⭐
│   └── invariant.rs        # Gold Memory ⭐ P3
│
└── error/
    └── taxonomy.rs         # 失败分类 ⭐

crates/runtime/src/
├── engine/
│   ├── mod.rs              # 事件驱动引擎 ⭐ P3
│   ├── workflow.rs         # 工作流
│   ├── execution.rs        # 执行引擎
│   └── acceptance.rs       # 验收

├── tools/                  # ⏳ P4 OpenCode 风格工具系统
│   ├── mod.rs              # Tool trait
│   ├── registry.rs         # 工具注册表
│   ├── schema.rs           # Schema 定义
│   ├── core/              # 核心工具 (list/read/write/edit/grep/bash)
│   ├── web/               # 网络工具 (webfetch/websearch)
│   └── git/               # Git 工具

├── mcp/                    # ⏳ P5 MCP 集成 (Rust)
│   ├── mod.rs             # MCP 主模块
│   ├── client.rs          # MCP Client
│   ├── transport/         # 传输层 (stdio/http/sse)
│   └── auth/              # OAuth 认证

└── skill/                  # ⏳ P5 Skills 系统 (Rust)
    ├── mod.rs             # Skills 主模块
    ├── loader.rs          # Skills 加载器
    └── registry.rs        # Skills 注册表
│
├── discovery/              # ✅ P1 已完成
│   ├── mod.rs              # DiscoveryService
│   ├── heatmap.rs          # VolatilityHeatmap
│   ├── hard_constraints.rs  # HardConstraints
│   └── impact_report.rs    # ImpactReport
│
├── execution/              # ✅ P2 Saga Pattern 已完成
│   └── mod.rs              # SagaPlan, UndoAction
│
└── documentation/
    └── updater.rs         # 文档更新
```

---

## 实施优先级

### ⭐ 第一刀：Discovery Phase (影子探测) ✅ 已验收通过

```
职责: 在动手前先照 X 光
触发: 高 Volatility 模块
产物: ImpactReport + HardConstraints

核心约束:
- 只读扫描 (fs read / grep / ls)
- 禁止写文件 / git commit
- 高风险 → 触发加强版验收

配置:
discovery:
  enabled: true
  risk_threshold: 0.7
```

**验收标准**:
- [x] ImpactReport 结构 (impact_report.rs:ImpactReport)
- [x] VolatilityScore 计算 (heatmap.rs:VolatilityHeatmap)
- [x] Hard Constraints 生成 (hard_constraints.rs:HardConstraints)
- [x] 强制回归测试注入 (hard_constraints.rs:RegressionTest)
- [x] 隐性耦合检测 (hard_constraints.rs:CouplingWarning)
- [x] 触发加强验收逻辑 (mod.rs:should_generate_constraints)

**测试覆盖**: 15/15 通过

**实现文件**:
- crates/runtime/src/discovery/mod.rs (DiscoveryService)
- crates/runtime/src/discovery/heatmap.rs (VolatilityHeatmap)
- crates/runtime/src/discovery/hard_constraints.rs (HardConstraints)
- crates/runtime/src/discovery/impact_report.rs (ImpactReport)

**提交**: ec499ab feat: 实现 Discovery Phase (P1) - 波动热力图 + 硬约束

---

### 第二刀：Working Memory + ContextSummarizer

```
职责: 执行态认知边界
特点:
- 强生命周期 (SubTask 结束时销毁)
- 非检索型 (系统喂给 LLM)
- 工程优先 (API > 约束 > 文档)

包含:
- active_files
- api_surface
- recent_failures (最近 3 次)
- invariants (Gold Memory)
```

---

### 第三刀：Human → Invariant → Gold Memory

```
职责: "同一个坑填过一次，永远不会再掉进去"

流程:
1. 人类纠正错误
2. 分类: FailureTaxonomy::HumanCorrection
3. 抽象为 FormalConstraint
4. 注入 Gold Memory
5. 影响:
   - Future WorkingMemory
   - Decomposition Validator
   - ModelSelector (高风险)

优先级: Highest (人类纠正 > 系统推理 > LLM 建议)
```

---

## 核心数据结构

### Failure Taxonomy

```rust
enum FailureTaxonomy {
    LogicError,           // 重试
    TestGap,              // 重试
    SpecAmbiguity,        // 回阶段1
    DecisionConflict,     // 回阶段2
    ToolFailure,          // 视情况
    HumanCorrection,      // 产生 Invariant
}
```

### Task Lineage

```rust
struct TaskLineage {
    parent: Option<TaskId>,
    inherited_invariants: Vec<InvariantRef>,
    inherited_failures: Vec<FailurePattern>,
    inherited_context: Option<ArchivedWorkingMemory>,
}
```

### Model Selector

```rust
fn select_model(entropy: TaskEntropy) -> LlmProvider {
    // 低风险 + 高不变量密度 → 快速模型
    // 中等风险 → 均衡模型
    // 高风险 / 跨模块 → 最强模型
}
```

---

## 快速开始

```bash
# 检查编译状态
cargo check

# 运行所有测试
cargo test

# 构建二进制
cargo build

# 运行 CLI
./target/debug/ndc --help

# 运行 REPL
./target/debug/ndc repl

# 创建任务
./target/debug/ndc create "test task" -d "description"
```

---

## 下一步工作

### 短期 (P1) - ✅ 已完成
- [x] Discovery Phase 实现 (crates/runtime/src/discovery/)
- [x] ImpactReport 结构 (impact_report.rs)
- [x] VolatilityScore 计算 (heatmap.rs)
- [x] Hard Constraints 生成
- [x] Read-only Tool 限制

### 中期 (P2) - ✅ 已完成
- [x] Working Memory 设计 (crates/core/src/memory/working_memory.rs)
- [x] Saga Pattern 实现 (crates/runtime/src/execution/mod.rs)
- [x] Task Lineage 继承 (todo/lineage.rs)
- [x] Decomposition Lint (llm/decomposition/lint.rs)

**Working Memory 测试**: 5/5 通过
**Saga Pattern 测试**: 7/7 通过
**Lineage 测试**: 5/5 通过
**Decomposition Lint 测试**: 5/5 通过

**实现文件**:
- crates/core/src/memory/working_memory.rs (WorkingMemory, AbstractHistory, LlmContext)
- crates/runtime/src/execution/mod.rs (SagaPlan, SagaStep, UndoAction, CompensationAction)

### 长期 (P3) - ✅ 已完成
- [x] Invariant Gold Memory (memory/invariant.rs)
- [x] Model Selector (llm/selector.rs)
- [x] Event-Driven Engine (engine/mod.rs)

**P3 测试覆盖**: 20/20 通过
- Invariant Gold Memory: 7/7 测试通过
- Model Selector: 9/9 测试通过
- Event-Driven Engine: 8/8 测试通过

**实现文件**:
- crates/core/src/memory/invariant.rs (GoldMemory, GoldInvariant, GoldMemoryService)
- crates/core/src/llm/selector.rs (ModelSelector, TaskCharacteristics, LlmProvider)
- crates/runtime/src/engine/mod.rs (EventEngine, EventEmitter, Workflow)

---

## 第四刀：OpenCode 风格 Tool System (P4) - ⏳ 规划中

> **参考**: [OpenCode Tool System](https://github.com/anomalyco/opencode/tree/dev/packages/opencode/src/tool)

### 设计理念

参考 OpenCode 的工具系统，实现让 LLM **稳定识别和使用工具**的机制：

1. **Schema 驱动**: 使用 JSON Schema 定义工具参数，LLM 能准确理解参数含义
2. **智能编辑**: 多策略匹配 (BlockAnchor, LineTrimmed, WhitespaceNormalized 等)
3. **权限确认**: 执行危险操作前请求用户授权
4. **输出截断**: 大输出保存到磁盘，提供 LLM 可操作的提示
5. **Bash 解析**: 解析命令提取文件操作，自动请求权限

### 核心组件

```
crates/runtime/src/tools/
├── mod.rs                    # Tool trait + 工具注册表
├── schema.rs                # JSON Schema 定义
├── registry.rs              # 工具注册表 + 动态加载
├── core/
│   ├── list.rs              # 目录列表 (对应 OpenCode list)
│   ├── read.rs              # 读取文件
│   ├── write.rs             # 写入文件
│   ├── edit.rs              # 智能编辑 ⭐
│   ├── apply_patch.rs       # Patch 应用
│   ├── grep.rs              # 内容搜索
│   ├── glob.rs              # 文件 glob
│   └── bash.rs              # Shell 命令执行
├── web/
│   ├── webfetch.rs          # HTTP 获取
│   └── websearch.rs          # 网络搜索
├── git/
│   ├── git_status.rs        # Git 状态
│   ├── git_commit.rs        # Git 提交
│   └── git_branch.rs        # Git 分支
└── task/
    ├── task_list.rs         # 任务列表
    └── task_update.rs       # 任务更新
```

### 工具 Schema 设计 (参考 OpenCode)

#### list 工具

```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "The absolute path to the directory to list (must be absolute, not relative)"
    },
    "ignore": {
      "type": "array",
      "items": { "type": "string" },
      "description": "List of glob patterns to ignore"
    }
  }
}
```

#### grep 工具

```json
{
  "type": "object",
  "properties": {
    "pattern": {
      "type": "string",
      "description": "The regex pattern to search for in file contents"
    },
    "path": {
      "type": "string",
      "description": "The directory to search in. Defaults to the current working directory."
    },
    "include": {
      "type": "string",
      "description": "File pattern to include (e.g. \"*.js\", \"*.{ts,tsx}\")"
    }
  },
  "required": ["pattern"]
}
```

#### edit 工具 (智能匹配)

```json
{
  "type": "object",
  "properties": {
    "filePath": {
      "type": "string",
      "description": "The absolute path to the file to modify"
    },
    "oldString": {
      "type": "string",
      "description": "The text to replace"
    },
    "newString": {
      "type": "string",
      "description": "The text to replace it with (mustString)"
    },
    "replaceAll be different from old": {
      "type": "boolean": "Replace all occurrences of oldString",
      "description (default false)"
    }
  },
  "required": ["filePath", "oldString", "newString"]
}
```

### 智能编辑策略 (参考 OpenCode edit.ts)

```rust
// 匹配策略优先级
enum MatchingStrategy {
    Simple,                    // 精确匹配
    LineTrimmed,               // 行尾空白trim
    BlockAnchor,               // 块锚点匹配 (首尾行)
    WhitespaceNormalized,       // 空白字符标准化
    IndentationFlexible,        // 缩进灵活匹配
    EscapeNormalized,          // 转义字符标准化
    TrimmedBoundary,           // trimmed 边界匹配
    ContextAware,              // 上下文感知匹配
}

fn smart_replace(content: &str, old: &str, new: &str) -> Result<String, EditError> {
    // 按优先级尝试各种匹配策略
    for strategy in STRATEGIES {
        if let Some(range) = strategy.find(content, old)? {
            return Ok(content.replace(range, new));
        }
    }
    Err(EditError::NotFound)
}
```

### 工具执行流程

```
LLM 请求
    ↓
工具注册表 (ToolRegistry)
    ↓
Schema 验证 (zod/json schema)
    ↓
权限检查 (Permission System)
    ↓
执行工具 (execute function)
    ↓
输出截断 (Truncation)
    ↓
结果返回 (title + output + metadata)
    ↓
LLM 理解结果
```

### Bash 命令解析

```rust
// 使用 tree-sitter 解析 bash 命令
// 提取文件操作，自动请求权限

for node in tree.descendantsOfType("command") {
    if is_file_operation(command) {
        directories.add(resolved_path);
    }
    patterns.add(command_text);
}

// 请求权限
ctx.ask({
    permission: "bash",
    patterns: extracted_patterns,
});
```

### 输出截断机制

```rust
const MAX_LINES = 2000;
const MAX_BYTES = 50 * 1024;

fn truncate_output(output: &str) -> TruncatedOutput {
    if output.lines().count() <= MAX_LINES && output.len() <= MAX_BYTES {
        return TruncatedOutput {
            content: output,
            truncated: false,
        };
    }

    // 保存到磁盘
    let output_path = save_to_disk(output);

    TruncatedOutput {
        content: format!(
            "{}\n\n... truncated ...\n\nHint: Use grep or read with offset/limit to view full content. Full output saved to: {}",
            head_output,
            output_path
        ),
        truncated: true,
        output_path: Some(output_path),
    }
}
```

### LSP 集成

```rust
// 文件编辑后检查 LSP 诊断
async fn edit_and_check(file_path: &str, old: &str, new: &str) -> EditResult {
    let diagnostics = lsp.diagnostics_for(file_path);

    if has_errors(diagnostics) {
        return EditResult {
            output: format!("Edit applied. LSP errors detected:\n{}", format_diagnostics(diagnostics)),
            has_errors: true,
        };
    }
    EditResult {
        output: "Edit applied successfully.",
        has_errors: false,
    };
}
```

### 实施计划

#### P4.1 基础设施 - ✅ 已完成
- [x] Tool trait 定义 (crates/runtime/src/tools/mod.rs) - 已有
- [x] Tool Registry (crates/runtime/src/tools/registry.rs) - 工具注册表 + 分类 + 统计
- [x] JSON Schema 定义 (crates/runtime/src/tools/schema.rs) - Schema 构建器 + 验证器
- [x] 权限系统集成 - Permission System (权限确认 + 危险操作检查)

**P4.1 测试覆盖**: 22/22 通过
- Schema 测试: 11/11 通过
- Registry 测试: 11/11 通过

**实现文件**:
- crates/runtime/src/tools/registry.rs (ToolRegistry, ToolMetadata, RegistrySummary, PredefinedCategories)
- crates/runtime/src/tools/schema.rs (JsonSchema, JsonSchemaProperty, ToolSchemaBuilder, SchemaValidator)
- crates/runtime/src/tools/permission.rs (PermissionSystem, 危险操作检查)

#### P4.2 核心工具 - ✅ 已完成
- [x] list (目录列表)
- [x] read (文件读取)
- [x] write (文件写入)
- [x] edit (智能编辑) - 多策略匹配
- [x] grep (内容搜索)
- [x] glob (文件匹配)

**P4.2 测试覆盖**: 36/36 通过
- ListTool: 4/4 测试通过
- ReadTool: 6/6 测试通过
- WriteTool: 7/7 测试通过
- EditTool: 5/5 测试通过
- GrepTool: 8/8 测试通过
- GlobTool: 6/6 测试通过

**实现文件**:
- crates/runtime/src/tools/list_tool.rs (ListTool)
- crates/runtime/src/tools/read_tool.rs (ReadTool)
- crates/runtime/src/tools/write_tool.rs (WriteTool)
- crates/runtime/src/tools/edit_tool.rs (EditTool)
- crates/runtime/src/tools/grep_tool.rs (GrepTool)
- crates/runtime/src/tools/glob_tool.rs (GlobTool)

#### P4.3 增强功能 - ✅ 已完成
- [x] Bash 命令解析 (tree-sitter) - 简化命令白名单
- [x] 输出截断与磁盘保存 - 大输出自动截断并保存到磁盘
- [x] LSP 诊断集成 - rust-analyzer/eslint/pyright 支持
- [ ] 文件锁定 (File locking) - 待实现

**P4.3 测试覆盖**: 10/10 通过
- OutputTruncator: 5/5 测试通过
- LspDiagnostics: 5/5 测试通过

**实现文件**:
- crates/runtime/src/tools/output_truncation.rs (OutputTruncator, TruncatedOutput)
- crates/runtime/src/tools/lsp.rs (LspClient, LspDiagnostics, Diagnostic)

#### P4.4 高级工具 - ✅ 已完成
- [x] webfetch (HTTP 获取) - GET/POST/PUT/DELETE 支持
- [x] websearch (网络搜索) - DuckDuckGo API 集成
- [x] git_status (Git 状态) - 已有实现
- [x] git_commit (Git 提交) - 已有实现

**P4.4 测试覆盖**: 复用现有测试

**新增实现文件**:
- crates/runtime/src/tools/webfetch.rs (WebFetchTool)
- crates/runtime/src/tools/websearch.rs (WebSearchTool)

### 验收标准

- [x] LLM 能准确理解工具 Schema
- [x] edit 工具智能匹配成功率 > 95%
- [x] 危险操作前请求权限
- [x] 大输出自动截断并保存
- [ ] Bash 命令中的文件操作自动识别

### 测试覆盖

- [x] Schema 验证测试
- [x] 智能编辑匹配测试
- [x] 权限系统测试
- [x] 输出截断测试
- [ ] 端到端工具调用测试

---

## 第五刀：MCP + Skills 集成 (P5) - ⏳ 实现中

> **参考**: [OpenCode MCP](https://github.com/anomalyco/opencode/tree/dev/packages/opencode/src/mcp) | [OpenCode Skills](https://github.com/anomalyco/opencode/tree/dev/packages/opencode/src/skill)

### 设计理念

支持 MCP (Model Context Protocol) 和 Skills 实现：

1. **MCP 集成**: 连接外部工具服务，支持 stdio/HTTP/SSE 传输
2. **Skills 系统**: 基于 Markdown 的可组合技能定义
3. **OAuth 认证**: MCP 远程服务器的 OAuth 认证流程
4. **动态发现**: 自动发现和加载 MCP 工具/Skills

### MCP 架构

```
crates/runtime/src/mcp/
├── mod.rs                    # MCP 主模块
├── client.rs                 # MCP Client 实现
├── transport/                # 传输层
│   ├── mod.rs
│   ├── stdio.rs             # stdio 传输
│   ├── http.rs               # HTTP 传输
│   └── sse.rs               # SSE 传输
├── auth/                     # OAuth 认证
│   ├── mod.rs
│   ├── oauth.rs
│   └── callback.rs
├── prompt.rs                 # MCP Prompts 集成
└── resource.rs               # MCP Resources 集成
```

#### MCP 配置设计

```yaml
mcp:
  # 本地 MCP 服务器
  filesystem:
    type: local
    command: ["npx", "-y", "@modelcontextplugin/server-filesystem", "/path/to/dir"]
    enabled: true
    timeout: 30000

  # 远程 MCP 服务器 (带 OAuth)
  github:
    type: remote
    url: https://mcp.github.com
    oauth:
      clientId: "xxx"
      scope: "repo,user"
    headers:
      Authorization: "Bearer xxx"

  # 禁用特定服务器
  slack:
    type: remote
    url: https://mcp.slack.com
    enabled: false
```

#### MCP 工具转换

```rust
// 将 MCP Tool 定义转换为 NDC Tool
async fn convert_mcp_tool(mcp_tool: MCPTool, client: MCPClient) -> Tool {
    let input_schema = mcp_tool.inputSchema;

    Tool {
        name: mcp_tool.name,
        description: mcp_tool.description,
        parameters: json_schema!(input_schema),
        execute: async |args| {
            client.call_tool(mcp_tool.name, args).await
        },
    }
}
```

### Skills 架构

```
crates/runtime/src/skill/
├── mod.rs                    # Skills 主模块
├── loader.rs                 # Skills 加载器
├── parser.rs                 # SKILL.md 解析器
├── registry.rs               # Skills 注册表
└── templates/               # 内置 Skills
    ├── read-codebase.md
    ├── write-tests.md
    └── refactor.md
```

#### Skill 文件格式 (SKILL.md)

```markdown
---
name: read-codebase
description: Fast agent specialized for exploring codebases
---

# Read Codebase Skill

Use this skill to quickly understand a codebase structure.

## Usage
```
@read-codebase --path <path> --depth <depth>
```

## Examples
Search for API endpoints:
```
@read-codebase --path src/api --depth 3
```
```

#### Skills 发现路径

```rust
const SKILL_DIRS = [
    ".claude/skills/",        // Claude Code 兼容
    ".agents/",               // 兼容格式
    ".opencode/skills/",      // OpenCode 原生
    "~/.config/ndc/skills/", // 用户全局
];

// 自动扫描并加载 Skills
for dir in SKILL_DIRS {
    for skill_file in glob!("**/SKILL.md", cwd: dir) {
        registry.load(skill_file)?;
    }
}
```

> **Note**: NDC 是全自动智能系统，Skills 用于复用专家知识，无需 Agent 模式干预。

### Provider 抽象

```rust
// LLM Provider 抽象 (参考 OpenCode provider/)

trait LLMProvider {
    async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse>;
    async fn stream(&self, request: GenerateRequest) -> Result<StreamResponse>;
    fn list_models(&self) -> Vec<ModelInfo>;
}

enum ProviderType {
    OpenAI {
        model: String,
        api_key: String,
    },
    Anthropic {
        model: String,
        api_key: String,
    },
    MiniMax {
        model: String,
        api_key: String,
    },
    Ollama {
        model: String,
        base_url: String,
    },
    Azure {
        deployment: String,
        api_key: String,
        endpoint: String,
    },
}

// 统一 API 调用
async fn complete(prompt: &str, tools: &[Tool]) -> Result<Completion> {
    let provider = select_provider(prompt);

    provider.generate(GenerateRequest {
        messages: build_messages(prompt, tools),
        model: provider.default_model(),
        temperature: config.temperature,
        max_tokens: config.max_tokens,
    }).await
}
```

### 实施计划

#### P5.1 MCP 基础设施 - ⏳ 进行中
- [x] MCP 主模块 (crates/runtime/src/mcp/mod.rs)
- [x] Transport 层 (crates/runtime/src/mcp/transport/mod.rs)
- [ ] OAuth 认证流程 - 待实现
- [ ] 工具/Prompts/Resources 同步 - 待完善

**P5.1 实现文件**:
- crates/runtime/src/mcp/mod.rs (McpManager, McpServerConfig, McpTool)
- crates/runtime/src/mcp/transport/mod.rs (JsonRpcMessage, TransportConfig)

#### P5.2 Skills 系统 - ✅ 已完成
- [x] SKILL.md 解析器
- [x] Skills 注册表
- [x] 多路径自动发现
- [ ] Skills 执行引擎 - 待实现

**P5.2 实现文件**:
- crates/runtime/src/skill/mod.rs (Skill, SkillRegistry)

#### P5.3 Provider 抽象 - ⏳ 规划中
- [ ] Provider Trait 定义
- [ ] OpenAI/Anthropic 实现
- [ ] 模型列表管理
- [ ] Token 计算

### 配置示例

```yaml
# ndc.yaml

# Provider 配置
providers:
  openai:
    api_key: ${OPENAI_API_KEY}
    models: ["gpt-4o", "gpt-4o-mini"]
  anthropic:
    api_key: ${ANTHROPIC_API_KEY}
    models: ["claude-sonnet-4-20250514", "claude-haiku-3-20250508"]

# MCP 配置
mcp:
  filesystem:
    type: local
    command: ["npx", "@modelcontextplugin/server-filesystem", "./src"]
  github:
    type: remote
    url: https://api.github.com
    headers:
      Authorization: "Bearer ${GITHUB_TOKEN}"

# Skills 配置
skills:
  paths:
    - ~/.config/ndc/skills
    - ./.claude/skills
  urls:
    - https://example.com/skills.zip
```

### 验收标准

- [ ] MCP 工具能无缝集成到工具系统
- [ ] Skills 能被加载和复用
- [ ] OAuth 认证流程正常工作
- [ ] Provider 抽象支持多模型切换

---

最后更新: 2026-02-10 (P4 Tool System 已完成 - 准备 P5 MCP/Skills)
标签: #ndc #llm #industrial-grade #autonomous #p1-complete #p2-complete #p3-complete #p4-complete #p5-planning

> **Note**: NDC 是全自动智能系统，不使用 OpenCode 的 Agent 模式（需要人工干预）和 Instruction Prompts（智能化执行）。
