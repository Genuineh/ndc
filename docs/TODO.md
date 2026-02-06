# NDC 实现待办清单

> **重要更新 (2026-02-05)**: 采用 **NDC 2.0 深度融合方案（第三轮优化）**
> - Git Worktree 快照
> - Verdict 权限等级（PrivilegeLevel）
> - Storage 懒加载（Stream + 分页）
> - 详情见: `docs/devman-integration-plan.md`

## 架构概览

```
ndc/
├── core/              # [核心] 统一模型 (Task-Intent 合一) ✅ 已更新
├── decision/          # [大脑] 决策引擎
├── cognition/         # [记忆] 认知网络 (原 DevMan Knowledge)
├── runtime/           # [身体] 执行与验证 (Tools + Quality)
├── persistence/       # [归档] 存储层（含事务+懒加载）
└── interface/         # [触觉] 交互层 (CLI + REPL + Daemon)
```

## ✅ Phase 1 + Phase 2 已完成

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| - | `crates/core/src/task.rs` | ✅ | GitWorktreeSnapshot + LightweightSnapshot |
| - | `crates/core/src/intent.rs` | ✅ | PrivilegeLevel + ConditionType |
| - | `crates/core/src/agent.rs` | ✅ | AgentRole, Permission |
| - | `crates/core/src/memory.rs` | ✅ | MemoryStability, MemoryQuery |
| - | `crates/persistence/src/store.rs` | ✅ | **Stream + 分页** |
| - | `crates/decision/src/engine.rs` | ✅ | DecisionEngine + PrivilegeLevel |
| - | `crates/runtime/` | ✅ | **完整执行引擎** |

---

## 第三轮优化：新增特性

### A. Git Worktree 快照 ✅

```rust
pub struct GitWorktreeSnapshot {
    pub worktree_path: PathBuf,      // 临时 worktree 路径
    pub base_commit: String,          // 基准 commit
    pub branch_name: String,           // 分支名
    pub affected_paths: Vec<PathBuf>, // 影响文件
}
```

**优势**：
- 精确回滚：删除 worktree，切回主分支
- 不污染主分支历史
- 支持任意粒度操作

### B. Verdict 权限等级 ✅

```rust
pub enum PrivilegeLevel {
    Normal = 0,     // 读文件、普通写
    Elevated = 1,   // 配置修改
    High = 2,       // 删除文件
    Critical = 3,   // 危险命令
}

Verdict::Allow {
    action: Action,
    privilege: PrivilegeLevel,  // 授予的权限
    conditions: Vec<Condition>,
}
```

**优势**：
- 精细控制：修改 Cargo.toml 需要 Elevated
- 权限不足返回 `ErrorCode::InsufficientPrivilege`
- 对接 AgentRole 权限系统

### C. Storage 懒加载 ✅

```rust
// Stream 方式（推荐）
fn search_memory_stream(
    &self,
    query: &str,
    min_stability: Option<MemoryStability>,
) -> Result<Pin<Box<dyn Stream<Item = Result<Memory>> + Send>>>;

// 分页方式
async fn search_memory_paged(
    &self,
    query: &MemoryQuery,
    offset: u64,
    limit: u64,
) -> Result<Vec<Memory>>;
```

**优势**：
- 万级 Memory 不吃内存
- 支持无限滚动
- 向量检索分页友好

---

## Phase 1: 内核重构 (Week 1) ✅ 已完成

| 任务 | 状态 | 说明 |
|------|------|------|
| `crates/core/src/task.rs` | ✅ | GitWorktreeSnapshot, LightweightSnapshot |
| `crates/core/src/intent.rs` | ✅ | PrivilegeLevel, ConditionType |
| `crates/core/src/agent.rs` | ✅ | AgentRole, Permission |
| `crates/core/src/memory.rs` | ✅ | MemoryStability, MemoryQuery |
| `crates/persistence/src/store.rs` | ✅ | Stream + 分页 |
| `crates/persistence/src/json.rs` | ✅ | JSON 实现 |
| `crates/decision/src/engine.rs` | ✅ | DecisionEngine + PrivilegeLevel |

---

## Phase 2: 执行层吸收 (Week 2) ✅ 已完成

### ndc-runtime 完整实现

```
crates/runtime/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── executor.rs       # 异步任务调度器
    ├── workflow.rs       # 状态机引擎
    ├── tools/
    │   ├── mod.rs       # ToolRegistry
    │   ├── trait_mod.rs # Tool trait
    │   ├── fs.rs        # FsTool（安全文件操作）
    │   ├── git.rs        # GitTool（worktree, commit）
    │   └── shell.rs      # ShellTool（白名单命令）
    └── verify/
        └── mod.rs        # QualityGateRunner
```

**特性**：
- `Executor`: 任务调度、质量门禁、工具协调
- `WorkflowEngine`: 状态机、转换规则、后置动作
- `FsTool`: 路径验证、防遍历攻击
- `GitTool`: worktree 快照、分支、提交
- `ShellTool`: 命令白名单、超时限制
- `QualityGateRunner`: 测试、lint、构建检查

---

## Phase 3: 认知升级 (Week 3) ✅ 已完成

### ndc-cognition 完整实现

```
crates/cognition/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── memory.rs       # 记忆存储 (L1/L2/L3)
    ├── vector.rs       # SimHash 语义搜索
    ├── stability.rs    # 稳定性管理
    └── context.rs      # 上下文组装
```

**特性**：

### MemoryStore
- CRUD 操作
- L1 缓存（LRU）
- 按稳定性/类型检索

### VectorSearch (SimHash)
- 轻量级向量计算
- 海明距离相似度
- 自动索引构建

### StabilityManager
- Ephemeral → Derived → Verified → Canonical
- 自动升级规则
- TTL 清理策略

### ContextBuilder
- 三层检索（L1/L2/L3）
- Token 预算控制
- 优先级排序

---

## Phase 4: 交互层 (Week 4) [P2]

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P2 | `crates/interface/Cargo.toml` | ☐ | 创建交互 crate |
| P2 | `crates/interface/src/cli.rs` | ☐ | CLI 入口 |
| P2 | `crates/interface/src/repl.rs` | ☐ | REPL 模式 |

---

## DevMan 迁移清单

| 来源 | 目标 | 状态 |
|------|------|------|
| devman-core | ndc-core | ✅ 已更新 |
| devman-storage | ndc-persistence | ✅ 已迁移 |
| devman-tools | ndc-runtime/tools | ✅ 已迁移 |
| devman-quality | ndc-runtime/verify | ✅ 已迁移 |
| devman-knowledge | ndc-cognition | ✅ 已迁移 |
| devman-work | ndc-runtime/workflow | ✅ 已迁移 |

---

## 核心原则检查

- [x] **向下引用**: `core` 是纯数据，不引用其他 crate
- [x] **Git Worktree**: 使用 worktree 做快照，支持精确回滚
- [x] **PrivilegeLevel**: Verdict 包含权限等级
- [x] **懒加载**: Stream + 分页查询
- [x] **事务**: 存储层支持 `Transaction` trait

---

## 迁移技巧

### 1. 先定义 Trait/结构，再迁代码
```rust
// 先在 core 里定义纯数据结构
pub struct Task { pub id: TaskId, pub state: TaskState, ... }
// 不要带方法，只留字段
```

### 2. 使用 #[serde(flatten)] 保留旧数据
```rust
#[derive(Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    #[serde(flatten)]
    pub legacy: HashMap<String, serde_json::Value>, // 旧字段临时保留
}
```

---

最后更新: 2026-02-05 (第三轮优化)
标签: #ndc #todo #integration
