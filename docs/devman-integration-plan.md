# NDC 2.0 深度融合方案（优化版）

**日期**: 2026-02-05
**状态**: 设计完成，待执行

---

## 核心原则：打破边界，器官化整合

放弃 `Adapter` 层概念，将 DevMan 功能拆散植入 NDC 架构。

---

## 1. 架构对比

### 整合前（Adapter 模式）
```
NDC → Adapter → DevMan → Tools
      (转换层)  (黑盒)
```

### 整合后（统一运行时）
```
┌─────────────────────────────────────────┐
│           NDC 统一运行时                  │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐ │
│  │ Decision │→│ Runtime │→│  Store  │ │
│  │  Engine │  │         │  │         │ │
│  └────┬────┘  └────┬────┘  └────┬────┘ │
│       │            │            │       │
│       └────────► Cognition ◄────┘       │
└─────────────────────────────────────────┘
```

---

## 2. 目录结构（最终版）

```text
ndc/
├── crates/
│   ├── core/              # [核心] 统一模型（纯数据定义）
│   │   ├── src/
│   │   │   ├── task.rs        # Task-Intent 统一结构（含 Snapshot）
│   │   │   ├── intent.rs      # Intent, Verdict, Effect
│   │   │   ├── agent.rs       # Agent, AgentRole, Permission
│   │   │   ├── memory.rs      # MemoryEntry, MemoryStability
│   │   │   └── lib.rs        # 模块入口 + Type Alias
│   │   └── Cargo.toml
│   │
│   ├── decision/          # [大脑] 决策引擎
│   │   ├── src/
│   │   │   ├── engine.rs      # DecisionEngine trait + 实现
│   │   │   ├── policy/        # 静态与动态策略
│   │   │   ├── validators.rs  # 内置校验器
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── cognition/         # [记忆] 认知网络
│   │   ├── src/
│   │   │   ├── vector.rs      # 向量检索 (#Issue 3)
│   │   │   ├── stability.rs   # 记忆稳定性演化 (#Issue 1)
│   │   │   ├── context.rs     # 实时上下文组装
│   │   │   ├── knowledge.rs   # Knowledge 结构
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── runtime/           # [身体] 执行与验证
│   │   ├── src/
│   │   │   ├── executor.rs    # 异步任务调度器
│   │   │   ├── tools/         # 受控工具集
│   │   │   │   ├── mod.rs
│   │   │   │   ├── fs.rs
│   │   │   │   ├── git.rs
│   │   │   │   ├── shell.rs
│   │   │   │   └── trait.rs
│   │   │   ├── verify/        # 质量门禁
│   │   │   │   ├── mod.rs
│   │   │   │   ├── tests.rs
│   │   │   │   └── lint.rs
│   │   │   ├── workflow.rs    # 状态机
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── persistence/       # [归档] 存储层
│   │   ├── src/
│   │   │   ├── store.rs       # 存储抽象 trait（核心）
│   │   │   ├── transaction.rs # 事务支持
│   │   │   ├── json.rs        # JSON 实现
│   │   │   ├── sqlite.rs      # SQLite 实现
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   └── interface/         # [触觉] 交互层
│       ├── src/
│       │   ├── cli.rs         # CLI 入口
│       │   ├── repl.rs        # REPL 交互
│       │   ├── daemon.rs      # gRPC 服务
│       │   └── lib.rs
│       └── Cargo.toml
│
└── docs/
    ├── README.md
    ├── TODO.md
    └── design/
        └── 2026-02-04-ndc-final-design.md
```

---

## 3. 核心原则：向下引用

### 3.1 依赖规则

```
       interface (CLI/REPL/Daemon)
              │
              ↓
         cognition ←──────┐
              │            │
              ↓            │
          runtime         │
              │            │
              ↓            │
         decision ────────┤
              │            │
              ↓            │
         ┌────┴────┐       │
         │  core   │ ←────┘
         │(纯数据) │
         └─────────┘
              │
              ↓
        persistence
```

**核心规则**：
1. `core` **不能**引用任何其他 crate（纯数据定义）
2. `decision`、`runtime`、`cognition` 可以引用 `core`
3. `persistence` 可以引用 `core`
4. `interface` 可以引用所有

### 3.2 Type Alias 策略

在迁移过程中，使用 Type Alias 避免大规模修改：

```rust
// ndc-core/src/lib.rs

// 核心类型重导出（兼容旧代码）
pub use crate::task::{Task, TaskState, TaskId};
pub use crate::intent::{Intent, Verdict, Action, Effect};
pub use crate::agent::{Agent, AgentRole, Permission};
pub use crate::memory::{Memory, MemoryStability, MemoryId};

// 迁移辅助类型（从 DevMan 兼容）
#[doc(hidden)]
pub type DevManTask = task::Task;

#[doc(hidden)]
pub type DevManKnowledge = knowledge::Knowledge;
```

---

## 4. 模块映射表

| DevMan 原模块 | 去向 | 整合后角色 |
|--------------|------|-----------|
| `devman-core/task.rs` | `ndc-core/task.rs` | **统一**：Task = 持久化 Intent |
| `devman-core/goal.rs` | `ndc-core/task.rs` | Goal 作为 Task 聚合 |
| `devman-knowledge/*` | `ndc-cognition/` | 认知网络 |
| `devman-quality/*` | `ndc-runtime/verify/` | 质量门禁 |
| `devman-tools/*` | `ndc-runtime/tools/` | 受控工具集 |
| `devman-work/*` | `ndc-runtime/workflow.rs` | 状态机 |
| `devman-storage/*` | `ndc-persistence/` | 持久化层 |
| `devman-ai/*` | `ndc-interface/` | 交互层 |
| `devman-progress/*` | `ndc-runtime/` | 融入执行器 |

---

## 5. 关键整合点

### 5.1 Task-Intent 统一

**新流程**：
```
Agent 提出 Intent → Decision Engine 裁决 → 持久化为 Task → Runtime 执行
```

### 5.2 决策 + 验证合体

```rust
impl DecisionEngine {
    pub async fn evaluate(&self, intent: Intent) -> Verdict {
        // ... 基础校验 ...

        // Verdict 可以包含前置条件
        if self.require_quality_check(&intent) {
            Verdict::AllowWithGate {
                action: intent.proposed_action,
                gate: QualityGate::from_intent(&intent),
            }
        } else {
            Verdict::Allow {
                action: intent.proposed_action,
            }
        }
    }
}
```

### 5.3 认知网络三层过滤

| 记忆层级 | 优先级 | 存储后端 | 稳定性策略 |
|---------|--------|---------|-----------|
| **L1: 工作空间** | 极高 | 内存 (LruCache) | 随 Task 销毁 |
| **L2: 语义网络** | 中 | 向量数据库 | 经 Reviewer 确认后固化 |
| **L3: 规范库** | 高 (强制) | Git / Markdown | 手动维护，作为 AI 的"法律" |

```rust
impl Cognition {
    pub async fn retrieve(&self, query: &str, min_stability: MemoryStability) -> Vec<Memory> {
        let candidates = self.vector_search(query).await;
        candidates
            .into_iter()
            .filter(|m| m.stability >= min_stability)
            .collect()
    }
}
```

---

## 6. 核心代码：Task 结构（含 Snapshot）

```rust
//! Task 模型 - Task 是持久化的 Intent
//!
//! 整合原则：
//! - Intent: AI 提出的行动提案
//! - Task: 已通过 Verdict 裁决的 Intent，可执行
//! - Task 包含原始 Intent 信息，保证可追溯性
//! - Task 包含 Snapshot，支持回滚

use crate::intent::{Intent, Action, Verdict};
use crate::agent::AgentRole;

/// 任务状态
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TaskState {
    Pending,              // 待处理
    Preparing,            // 准备中
    InProgress,           // 进行中
    AwaitingVerification, // 等待验证
    Blocked,              // 被阻塞
    Completed,            // 已完成
    Failed,               // 失败
    Cancelled,            // 已取消
}

/// 任务快照（支持回滚）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskSnapshot {
    /// 快照 ID
    pub id: SnapshotId,
    /// 捕获时间
    pub captured_at: Timestamp,
    /// Git Commit Hash（如适用）
    pub git_commit: Option<String>,
    /// 受影响文件列表（路径 + hash）
    pub affected_files: Vec<FileSnapshot>,
    /// 内存快照（Base64 编码，如 state.json）
    pub memory_snapshot: Option<String>,
    /// 创建者
    pub created_by: AgentRole,
}

/// 单文件快照
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content_hash: String,  // SHA256
    pub size: u64,
}

/// 任务
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    /// 任务 ID
    pub id: TaskId,

    /// 原始 Intent（追溯来源）
    pub intent: Intent,

    /// 裁决结果（证明合法性）
    pub verdict: Verdict,

    /// 当前状态
    pub state: TaskState,

    /// 允许的状态转换
    #[serde(default)]
    pub allowed_transitions: Vec<TaskState>,

    /// 执行步骤
    #[serde(default)]
    pub steps: Vec<ExecutionStep>,

    /// 质量门禁
    pub quality_gate: Option<QualityGate>,

    /// 元数据
    pub metadata: TaskMetadata,

    /// 任务快照（Preparing 阶段捕获）
    #[serde(default)]
    pub snapshots: Vec<TaskSnapshot>,
}

/// 执行步骤
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionStep {
    pub step_id: u64,
    pub action: Action,
    pub status: StepStatus,
    pub result: Option<ActionResult>,
    pub executed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metrics: ActionMetrics,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ActionMetrics {
    pub duration_ms: u64,
    pub tokens_used: u64,
    pub memory_access: Vec<MemoryId>,
}

/// 质量门禁
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityGate {
    pub checks: Vec<QualityCheck>,
    pub strategy: GateStrategy,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GateStrategy {
    FailFast,
    AllMustPass,
    Weighted,
}

/// 质量检查
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityCheck {
    pub check_type: QualityCheckType,
    pub command: Option<String>,
    pub pass_condition: PassCondition,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum QualityCheckType {
    Test,
    Lint,
    TypeCheck,
    Build,
    Custom(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PassCondition {
    ExitCode(u32),
    RegexMatch(String),
    OutputContains(String),
}

impl Task {
    /// 创建新任务
    pub fn from_intent_and_verdict(intent: Intent, verdict: Verdict) -> Self {
        Self {
            id: TaskId::new(),
            intent,
            verdict,
            state: TaskState::Pending,
            allowed_transitions: Self::initial_transitions(),
            steps: vec![],
            quality_gate: None,
            metadata: TaskMetadata::default(),
            snapshots: vec![],
        }
    }

    fn initial_transitions() -> Vec<TaskState> {
        vec![TaskState::Preparing]
    }

    /// 请求状态转换
    pub fn request_transition(&mut self, to: TaskState) -> Result<(), TransitionError> {
        if !self.allowed_transitions.contains(&to) {
            return Err(TransitionError::NotAllowed {
                from: self.state.clone(),
                to,
            });
        }
        self.state = to;
        self.update_allowed_transitions();
        Ok(())
    }

    /// 更新允许的转换
    fn update_allowed_transitions(&mut self) {
        self.allowed_transitions = match self.state {
            TaskState::Pending => vec![TaskState::Preparing],
            TaskState::Preparing => vec![TaskState::InProgress],
            TaskState::InProgress => vec![
                TaskState::AwaitingVerification,
                TaskState::Blocked,
            ],
            TaskState::AwaitingVerification => vec![
                TaskState::Completed,
                TaskState::Failed,
                TaskState::InProgress,
            ],
            TaskState::Blocked => vec![TaskState::InProgress],
            _ => vec![],
        };
    }

    /// 捕获当前快照（用于回滚）
    pub fn capture_snapshot(&mut self, files: Vec<FileSnapshot>) {
        self.snapshots.push(TaskSnapshot {
            id: SnapshotId::new(),
            captured_at: Timestamp::now(),
            git_commit: None,
            affected_files: files,
            memory_snapshot: None,
            created_by: self.metadata.created_by.clone(),
        });
    }

    /// 获取最新快照
    pub fn latest_snapshot(&self) -> Option<&TaskSnapshot> {
        self.snapshots.last()
    }
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("不允许的状态转换: {from:?} -> {to:?}")]
    NotAllowed { from: TaskState, to: TaskState },
}

// 类型别名
pub type TaskId = ulid::Ulid;
pub type SnapshotId = ulid::Ulid;
pub type Timestamp = chrono::DateTime<chrono::Utc>;
```

---

## 7. 核心代码：存储 Trait（支持原子写入）

```rust
//! 存储抽象层
//!
//! 设计原则：
//! - 支持事务（Transaction）
//! - 支持原子写入（Atomic Write）
//! - 零拷贝读取（返回引用）

use crate::core::{Task, TaskId, Memory, MemoryId};
use std::path::PathBuf;

/// 存储错误
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("记录不存在: {0}")]
    NotFound(String),

    #[error("事务冲突: {0}")]
    TransactionConflict(String),

    #[error("并发锁定失败: {0}")]
    LockFailed(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// 存储抽象 Trait
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    /// 打开存储
    async fn open(path: &PathBuf) -> Result<Self>
    where
        Self: Sized;

    /// 关闭存储
    async fn close(&mut self) -> Result<()>;

    // ============ Task 操作 ============

    /// 保存任务（原子写入）
    async fn save_task(&self, task: &Task) -> Result<()>;

    /// 获取任务
    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>>;

    /// 删除任务
    async fn delete_task(&self, id: &TaskId) -> Result<()>;

    /// 列出所有任务
    async fn list_tasks(&self) -> Result<Vec<TaskId>>;

    // ============ Memory 操作 ============

    /// 保存记忆
    async fn save_memory(&self, memory: &Memory) -> Result<()>;

    /// 获取记忆
    async fn get_memory(&self, id: &MemoryId) -> Result<Option<Memory>>;

    /// 搜索记忆
    async fn search_memory(&self, query: &str) -> Result<Vec<Memory>>;

    /// 删除记忆
    async fn delete_memory(&self, id: &MemoryId) -> Result<()>;

    // ============ 事务支持 ============

    /// 开始事务
    async fn begin_transaction(&self) -> Result<Transaction>;

    /// 提交所有挂起的写入
    async fn commit(&self) -> Result<()>;

    /// 回滚所有挂起的写入
    async fn rollback(&self) -> Result<()>;
}

/// 事务
#[async_trait::async_trait]
pub trait Transaction: Send {
    /// 保存任务
    async fn save_task(&mut self, task: &Task) -> Result<()>;

    /// 删除任务
    async fn delete_task(&mut self, id: &TaskId) -> Result<()>;

    /// 保存记忆
    async fn save_memory(&mut self, memory: &Memory) -> Result<()>;

    /// 删除记忆
    async fn delete_memory(&mut self, id: &MemoryId) -> Result<()>;

    /// 提交事务
    async fn commit(self) -> Result<()>;

    /// 回滚事务
    async fn rollback(self) -> Result<()>;
}

/// 批处理操作（性能优化）
#[async_trait::async_trait]
pub trait BatchStorage: Storage {
    /// 批量保存任务
    async fn save_tasks_batch(&self, tasks: &[Task]) -> Result<()>;

    /// 批量保存记忆
    async fn save_memories_batch(&self, memories: &[Memory]) -> Result<()>;
}
```

---

## 8. 执行计划

### Phase 1: 内核重构 (Week 1) [P0]

| 优先级 | 任务 | 说明 |
|--------|------|------|
| P0 | 创建 `ndc-core` | 纯数据定义 |
| P0 | `crates/core/src/task.rs` | **含 Snapshot** |
| P0 | `crates/core/src/intent.rs` | Intent, Verdict, Effect |
| P0 | `crates/persistence/src/store.rs` | **含事务** |
| P0 | 整合 devman-core | 迁移数据模型 |

### Phase 2: 执行层吸收 (Week 2) [P0]

| 优先级 | 任务 | 说明 |
|--------|------|------|
| P0 | 创建 `ndc-runtime` | 整合 Tools + Quality |
| P0 | `ndc-runtime/tools/` | 受控工具集 |
| P0 | `ndc-runtime/verify/` | 质量门禁 |
| P0 | 重构决策链路 | Decision → Runtime |

### Phase 3: 认知升级 (Week 3) [P1]

| 优先级 | 任务 | 说明 |
|--------|------|------|
| P1 | 创建 `ndc-cognition` | 三层过滤架构 |
| P1 | 向量检索 (#Issue 3) | LanceDB 集成 |
| P1 | 稳定性 (#Issue 1) | L1/L2/L3 分层 |

### Phase 4: 交互层 (Week 4) [P2]

---

## 9. 迁移策略

### 第一步：Type Alias 兼容
```rust
// 在 ndc-core/src/lib.rs 中
pub type Task = task::Task;
pub type Intent = intent::Intent;
// ... 其他类型
```

### 第二步：逐步替换
1. 创建新文件（ndc-core/src/*.rs）
2. 复制 devman 代码
3. 删除 devman 依赖
4. 修复编译错误

---

## 10. 优势总结

| 方面 | 改进 |
|------|------|
| **性能** | 零序列化，内存传递 |
| **架构** | 统一运行时，无 Adapter |
| **依赖** | 向下引用，无循环 |
| **回滚** | Snapshot 支持 |
| **事务** | 原子写入 |
| **可追溯性** | 每个 Task 带 Verdict |

---

## 相关文档

- `docs/README.md` - 文档导航
- `docs/TODO.md` - 实现清单

---

最后更新: 2026-02-05 (优化版)
