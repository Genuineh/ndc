# NDC 完整工程约束流程设计

> 整合知识库 + TODO 管理 + 任务分解 + 开发验收

---

## 1. 核心设计理念

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          NDC 知识驱动开发流程                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    知识库 (Knowledge Base)                         │   │
│  │  - 项目文档 (README, ARCHITECTURE, API Docs)                     │   │
│  │  - 代码知识 (CodeKnowledge)                                        │   │
│  │  - 决策记录 (Decision Records)                                     │   │
│  │  - 变更历史 (Change History)                                       │   │
│  │  - 稳定性层级: Ephemeral → Derived → Verified → Canonical        │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                      │
│                                    ▼                                      │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    TODO 管理 (Task Mapping)                        │   │
│  │  - 总 TODO (Project-Level)                                        │   │
│  │  - 子 TODO (Feature-Level)                                        │   │
│  │  - 任务分解链 (Task Chain)                                         │   │
│  │  - 状态追踪 (State Tracking)                                       │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                      │
│                                    ▼                                      │
│  ┌───────────────────────────────────────────────────────────────────┐   │
│  │                    任务执行 (Execution)                           │   │
│  │  - 步骤分解 (Step Decomposition)                                  │   │
│  │  - 开发执行 (Development)                                          │   │
│  │  - 测试验证 (Testing)                                              │   │
│  │  - 质量门禁 (Quality Gate)                                        │   │
│  │  - 验收确认 (Acceptance)                                          │   │
│  └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 完整流程状态机

```
                              ┌─────────────────────────────────────────┐
                              │           用户提出需求                    │
                              └─────────────────┬───────────────────────┘
                                                │
                                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│   ┌───────────────────────────────────────────────────────────────────┐    │
│   │                    阶段 1: 理解需求 (Understand)                   │    │
│   │  - 检索知识库                                                      │    │
│   │  - 检索总 TODO                                                     │    │
│   │  - 检查是否已有映射                                                  │    │
│   │  - 输出: RequirementContext                                        │    │
│   └───────────────────────────────────────────────────────────────────┘    │
│                                     │                                      │
│                                     ▼                                      │
│                         总 TODO 映射存在?                                    │
│                         ┌──────────┴──────────┐                          │
│                         ▼                        ▼                          │
│               ┌──────────────────┐    ┌──────────────────────────┐        │
│               │ 是: 关联现有      │    │ 否: 建立新映射             │        │
│               │    TODO          │    │    (提示用户确认)          │        │
│               └────────┬─────────┘    └────────────┬─────────────┘        │
│                        │                          │                       │
│                        │                          ▼                       │
│                        │               ┌──────────────────────────┐        │
│                        │               │  阶段 2a: 建立 TODO 映射   │        │
│                        │               │  - 创建总 TODO 条目        │        │
│                        │               │  - 关联用户需求            │        │
│                        │               │  - 标记为 pending          │        │
│                        │               └────────────┬─────────────┘        │
│                        │                          │                       │
│                        ▼                          │                       │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 2: 分解需求 (Decompose)                          │   │
│   │  - LLM 理解需求                                                   │   │
│   │  - 检索相关文档和知识                                               │   │
│   │  - 分解为原子子任务                                                 │   │
│   │  - 创建子 TODO 链                                                  │   │
│   │  - 记录任务依赖和顺序                                               │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 3: 执行开发 (Develop)                            │   │
│   │  ┌───────────────────────────────────────────────────────────┐   │   │
│   │  │ 子任务循环:                                                 │   │   │
│   │  │  3.1 开发执行 - 编写代码                                     │   │   │
│   │  │  3.2 单元测试 - 编写/运行测试                                │   │   │
│   │  │  3.3 质量门禁 - cargo check/test/clippy                     │   │   │
│   │  │  3.4 验证结果 - 通过/失败                                    │   │   │
│   │  │          ↓ 失败                                               │   │   │
│   │  │  3.5 重来机制 - 最多 N 次                                    │   │   │
│   │  │          ↓ 超过阈值                                           │   │   │
│   │  │  3.6 人工介入 - 暂停/报告                                     │   │   │
│   │  └───────────────────────────────────────────────────────────┘   │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                           所有子任务完成?                                    │
│                           ┌──────────┴──────────┐                        │
│                           ▼                        ▼                        │
│                 ┌──────────────────┐    ┌──────────────────────────┐        │
│                 │ 是              │    │ 否: 继续执行             │        │
│                 └────────┬─────────┘    └────────────────────────┘        │
│                          │                                               │
│                          ▼                                               │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 4: 验收 (Accept)                               │   │
│   │  - 自动验收: 覆盖率 >= 80%, 所有测试通过                           │   │
│   │  - 人工验收: 需要复核的业务逻辑                                     │   │
│   │  - 输出: AcceptanceResult                                        │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                          验收通过?                                          │
│                          ┌──────────┴──────────┐                          │
│                          ▼                        ▼                        │
│                ┌──────────────────┐    ┌──────────────────────────┐      │
│                │ 是              │    │ 否: 修复/重来              │      │
│                └────────┬─────────┘    └────────────────────────┘      │
│                         │                                                  │
│                         ▼                                                  │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 5: 更新文档 (Document)                          │   │
│   │  - 更新相关文档                                                    │   │
│   │  - 记录决策和变更                                                  │   │
│   │  - 提升知识库稳定性                                                 │   │
│   │  - 输出: DocumentChanges                                          │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                     │                                      │
│                                     ▼                                      │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │              阶段 6: 完成 (Complete)                              │   │
│   │  - 标记总 TODO 完成                                                │   │
│   │  - 发送完成通知                                                    │   │
│   │  - 输出: CompletionReport                                        │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 数据结构设计

### 3.1 总 TODO 结构 (Project-Level)

```rust
// crates/core/src/todo/project_todo.rs

/// 总 TODO - 项目级需求追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTodo {
    /// 总 TODO ID
    pub id: TodoId,

    /// 需求标题
    pub title: String,

    /// 需求描述 (原始用户输入)
    pub description: String,

    /// 需求来源
    pub source: RequirementSource,

    /// 状态
    pub state: TodoState,

    /// 优先级
    pub priority: TodoPriority,

    /// 关联的子 TODO 链
    pub task_chain: TaskChain,

    /// 元数据
    pub metadata: TodoMetadata,

    /// 创建时间
    pub created_at: Timestamp,

    /// 更新时间
    pub updated_at: Timestamp,

    /// 完成时间
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TodoState {
    /// 待处理 - 刚创建
    Pending,

    /// 理解中 - 正在理解需求
    Understanding,

    /// 分解中 - 正在分解任务
    Decomposing,

    /// 开发中 - 子任务执行中
    InProgress,

    /// 验收中 - 待验收
    AwaitingAcceptance,

    /// 文档更新中
    Documenting,

    /// 已完成
    Completed,

    /// 失败
    Failed,

    /// 已取消
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequirementSource {
    /// 用户输入
    UserInput(String),

    /// 需求文档
    Document { path: String, line: u32 },

    /// 会议纪要
    MeetingNotes { date: Date, participants: Vec<String> },

    /// GitHub Issue
    GitHubIssue { owner: String, repo: String, number: u32 },

    /// 其他
    Other(String),
}

/// 子任务链
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskChain {
    /// 根任务 ID
    pub root_task_id: TaskId,

    /// 子任务列表 (按执行顺序)
    pub subtasks: Vec<SubTask>,

    /// 依赖图
    pub dependencies: DependencyGraph,

    /// 当前执行位置
    pub current_position: usize,
}

/// 子任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    /// 子任务 ID
    pub id: SubTaskId,

    /// 标题
    pub title: String,

    /// 描述
    pub description: String,

    /// 状态
    pub subtask_state: SubTaskState,

    /// 优先级
    pub priority: TodoPriority,

    /// 验收标准
    pub acceptance_criteria: Vec<String>,

    /// 负责的 Agent
    pub assignee: AgentId,

    /// 依赖的子任务 ID
    pub depends_on: Vec<SubTaskId>,

    /// 实际步骤 (由 LLM 分解)
    pub steps: Vec<TaskStep>,

    /// 执行结果
    pub result: Option<SubTaskResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubTaskState {
    /// 待执行
    Pending,

    /// 准备中
    Preparing,

    /// 执行中
    InProgress,

    /// 验证中
    Verifying,

    /// 已完成
    Completed,

    /// 失败
    Failed,

    /// 跳过
    Skipped,
}
```

### 3.2 知识库集成结构

```rust
// crates/core/src/memory/knowledge_base.rs

/// 知识库 - 项目文档和代码知识管理
#[derive(Debug, Clone)]
pub struct KnowledgeBase {
    /// 文档索引
    documents: DocumentIndex,

    /// 代码知识
    code_knowledge: CodeKnowledgeIndex,

    /// 决策记录
    decisions: DecisionRecordIndex,

    /// 变更历史
    change_history: ChangeHistory,
}

impl KnowledgeBase {
    /// 检索相关文档
    pub async fn retrieve_documents(
        &self,
        query: &str,
    ) -> Result<Vec<DocumentRef>, KnowledgeError> {
        // 1. 搜索项目文档
        let docs = self.documents.search(query).await?;

        // 2. 搜索代码知识
        let code = self.code_knowledge.search(query).await?;

        // 3. 搜索决策记录
        let decisions = self.decisions.search(query).await?;

        Ok(vec![docs, code, decisions].concat())
    }

    /// 检查需求是否已存在
    pub async fn check_requirement_exists(
        &self,
        requirement: &str,
    ) -> Result<Option<ProjectTodoRef>, KnowledgeError> {
        // 1. 检查总 TODO
        let existing = self.todo_index.find_similar(requirement).await?;

        Ok(existing)
    }

    /// 更新文档
    pub async fn update_document(
        &self,
        path: &Path,
        changes: &DocumentChanges,
    ) -> Result<DocumentVersion, KnowledgeError> {
        // 1. 读取当前文档
        let current = self.read_document(path).await?;

        // 2. 应用变更
        let updated = self.apply_changes(&current, changes)?;

        // 3. 写入新版本
        let version = self.write_version(path, &updated).await?;

        // 4. 更新索引
        self.documents.update_index(path, &version).await?;

        // 5. 提升稳定性层级
        self.promote_stability(path, MemoryStability::Canonical)?;

        Ok(version)
    }

    /// 记录决策
    pub async fn record_decision(
        &self,
        decision: &DecisionRecord,
    ) -> Result<(), KnowledgeError> {
        // 决策初始为 Verified，后续人工复核后为 Canonical
        self.decisions.store(decision, MemoryStability::Verified)?;
        Ok(())
    }
}

/// 文档变更
#[derive(Debug, Clone)]
pub struct DocumentChanges {
    /// 文档路径
    pub path: PathBuf,

    /// 变更类型
    pub change_type: ChangeType,

    /// 变更内容
    pub content: Option<String>,

    /// 变更原因
    pub reason: String,

    /// 关联的 TODO
    pub todo_id: Option<TodoId>,

    /// 变更摘要
    pub summary: String,
}
```

---

## 4. LLM 集成设计

### 4.1 LLM 接口设计

```rust
// crates/core/src/llm/mod.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// LLM Provider Trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送聊天消息
    async fn chat(&self, messages: &[LlmMessage]) -> Result<LlmResponse, LlmError>;

    /// 流式聊天
    async fn chat_stream(
        &self,
        messages: &[LlmMessage],
    ) -> Result<impl Stream<Item = Result<String, LlmError>>, LlmError>;

    /// 健康检查
    async fn is_healthy(&self) -> bool;

    /// 获取模型名称
    fn model_name(&self) -> &str;
}

/// LLM 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// LLM 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub usage: TokenUsage,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
```

### 4.2 需求理解服务

```rust
// crates/core/src/llm/requirement_understanding.rs

/// 需求理解服务 - 阶段 1
pub struct RequirementUnderstandingService {
    llm_provider: Arc<dyn LlmProvider>,
    knowledge_base: Arc<KnowledgeBase>,
    todo_manager: Arc<TodoManager>,
}

impl RequirementUnderstandingService {
    /// 理解用户需求
    pub async fn understand(
        &self,
        user_input: &str,
        context: &UnderstandingContext,
    ) -> Result<UnderstandingResult, UnderstandingError> {
        // 1. 检索知识库
        let knowledge_context = self
            .knowledge_base
            .retrieve_documents(user_input)
            .await?;

        // 2. 检查总 TODO
        let existing_mapping = self
            .todo_manager
            .find_similar_todo(user_input)
            .await?;

        // 3. LLM 分析需求
        let system_prompt = self.build_system_prompt(&knowledge_context);

        let messages = vec![
            LlmMessage {
                role: MessageRole::System,
                content: system_prompt,
            },
            LlmMessage {
                role: MessageRole::User,
                content: format!(
                    "用户需求: {}\n\n请分析这个需求:\n1. 核心功能是什么?\n2. 涉及哪些模块?\n3. 是否有现有 TODO 映射? {:?}",
                    user_input,
                    existing_mapping.as_ref().map(|t| t.id.to_string())
                ),
            },
        ];

        let response = self.llm_provider.chat(&messages).await?;

        // 4. 解析结果
        let analysis = self.parse_analysis(&response.content)?;

        Ok(UnderstandingResult {
            requirement: user_input.to_string(),
            core_features: analysis.core_features,
            involved_modules: analysis.involved_modules,
            existing_todo_ref: existing_mapping,
            knowledge_context,
        })
    }

    fn build_system_prompt(&self, knowledge: &KnowledgeContext) -> String {
        format!(
            r#"你是一个专业的技术需求分析助手。
请结合以下项目背景知识分析用户需求:

项目文档:
{}

代码知识:
{}

决策记录:
{}

请分析用户需求的核心功能、涉及模块，并检查是否已有相关 TODO 映射。
"#,
            knowledge.documents,
            knowledge.code_knowledge,
            knowledge.decisions
        )
    }
}

/// 理解结果
#[derive(Debug)]
pub struct UnderstandingResult {
    pub requirement: String,
    pub core_features: Vec<String>,
    pub involved_modules: Vec<String>,
    pub existing_todo_ref: Option<ProjectTodoRef>,
    pub knowledge_context: KnowledgeContext,
}
```

### 4.3 TODO 映射服务

```rust
// crates/core/src/todo/mapping_service.rs

/// TODO 映射服务 - 阶段 2
pub struct TodoMappingService {
    todo_manager: Arc<TodoManager>,
    notifier: Arc<NotificationService>,
}

impl TodoMappingService {
    /// 检查并建立 TODO 映射
    pub async fn check_or_create_mapping(
        &self,
        requirement: &str,
        understanding: &UnderstandingResult,
    ) -> Result<TodoMappingResult, TodoError> {
        // 1. 检查是否已有映射
        if let Some(existing) = &understanding.existing_todo_ref {
            return Ok(TodoMappingResult {
                is_new: false,
                todo_id: existing.id,
                message: format!("已关联到现有 TODO: {}", existing.title),
            });
        }

        // 2. 创建新映射
        let new_todo = ProjectTodo {
            id: TodoId::new(),
            title: self.extract_title(requirement),
            description: requirement.to_string(),
            source: RequirementSource::UserInput(requirement.to_string()),
            state: TodoState::Understanding,
            priority: TodoPriority::Medium,
            task_chain: TaskChain::new(),
            metadata: TodoMetadata::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };

        self.todo_manager.create(&new_todo).await?;

        // 3. 通知用户
        self.notifier.notify(&Notification {
            kind: NotificationKind::TodoCreated,
            title: "新需求已创建 TODO",
            message: format!("需求已映射到 TODO: {}", new_todo.title),
            todo_id: Some(new_todo.id),
        }).await;

        Ok(TodoMappingResult {
            is_new: true,
            todo_id: new_todo.id,
            message: "已创建新的 TODO 映射，请确认需求描述是否准确".to_string(),
        })
    }
}
```

### 4.4 任务分解服务

```rust
// crates/core/src/llm/task_decomposition.rs

/// 任务分解服务 - 阶段 3
pub struct TaskDecompositionService {
    llm_provider: Arc<dyn LlmProvider>,
    knowledge_base: Arc<KnowledgeBase>,
    todo_manager: Arc<TodoManager>,
}

impl TaskDecompositionService {
    /// 分解需求为子任务
    pub async fn decompose(
        &self,
        requirement: &str,
        todo_id: TodoId,
        context: &UnderstandingContext,
    ) -> Result<DecompositionResult, DecompositionError> {
        // 1. 更新状态为 Decomposing
        self.todo_manager
            .update_state(todo_id, TodoState::Decomposing)
            .await?;

        // 2. 检索相关文档和代码知识
        let knowledge = self
            .knowledge_base
            .retrieve_documents(requirement)
            .await?;

        // 3. LLM 分解
        let system_prompt = self.build_decomposition_prompt(&knowledge);

        let messages = vec![
            LlmMessage {
                role: MessageRole::System,
                content: system_prompt,
            },
            LlmMessage {
                role: MessageRole::User,
                content: format!(
                    "请将以下需求分解为原子子任务:\n\n{}\n\n\
                    要求:\n\
                    1. 每个子任务必须是独立可执行的\n\
                    2. 每个子任务必须有明确的验收标准\n\
                    3. 考虑任务之间的依赖关系\n\
                    4. 参考项目文档: {}",
                    requirement,
                    knowledge.summarize()
                ),
            },
        ];

        let response = self.llm_provider.chat(&messages).await?;

        // 4. 解析为 TaskChain
        let task_chain = self.parse_task_chain(&response.content)?;

        // 5. 保存到 TODO
        self.todo_manager
            .update_task_chain(todo_id, &task_chain)
            .await?;

        // 6. 更新状态
        self.todo_manager
            .update_state(todo_id, TodoState::InProgress)
            .await?;

        Ok(DecompositionResult {
            todo_id,
            task_chain,
            subtask_count: task_chain.subtasks.len(),
        })
    }

    fn build_decomposition_prompt(&self, knowledge: &KnowledgeContext) -> String {
        format!(
            r#"你是一个专业的技术任务分解助手。
请将用户需求分解为原子子任务。

项目背景:
{}

代码结构:
{}

分解原则:
1. 每个子任务应该是原子性的，可独立完成
2. 每个子任务必须有:
   - 清晰的标题
   - 详细的描述
   - 明确的验收标准 (至少 3 条)
   - 预估的复杂度 (简单/中等/复杂)
3. 考虑文件依赖和模块依赖
4. 遵循测试驱动开发: 每个功能任务后应跟随测试任务

输出格式 (JSON):
{{
  "subtasks": [
    {{
      "title": "子任务标题",
      "description": "详细描述",
      "acceptance_criteria": ["标准1", "标准2", "标准3"],
      "complexity": "simple|medium|complex",
      "estimated_steps": 3,
      "depends_on": []
    }}
  ],
  "execution_order": [[0], [1, 2], [3]]  # 可并行/串行执行
}}
"#,
            knowledge.documents,
            knowledge.code_structure
        )
    }
}
```

---

## 5. 完整执行引擎

```rust
// crates/runtime/src/engine/workflow_engine.rs

/// NDC 工作流引擎 - 整合所有阶段
pub struct NdcWorkflowEngine {
    /// 需求理解服务
    understanding_service: Arc<RequirementUnderstandingService>,

    /// TODO 映射服务
    mapping_service: Arc<TodoMappingService>,

    /// 任务分解服务
    decomposition_service: Arc<TaskDecompositionService>,

    /// 步骤执行引擎
    step_engine: Arc<StepExecutionEngine>,

    /// 验收服务
    acceptance_service: Arc<AcceptanceService>,

    /// 文档更新服务
    documentation_service: Arc<DocumentationService>,

    /// 通知服务
    notifier: Arc<NotificationService>,
}

impl NdcWorkflowEngine {
    /// 执行完整工作流
    pub async fn execute_workflow(
        &self,
        user_input: &str,
    ) -> Result<WorkflowResult, WorkflowError> {
        // ===== 阶段 1: 理解需求 =====
        let understanding = self
            .understanding_service
            .understand(user_input, &UnderstandingContext::default())
            .await?;

        // ===== 阶段 2: 检查/建立 TODO 映射 =====
        let mapping = self
            .mapping_service
            .check_or_create_mapping(user_input, &understanding)
            .await?;

        // ===== 阶段 3: 分解需求 =====
        let decomposition = self
            .decomposition_service
            .decompose(user_input, mapping.todo_id, &understanding.context())
            .await?;

        // ===== 阶段 4: 执行子任务 =====
        let execution_result = self
            .execute_subtasks(mapping.todo_id, &decomposition.task_chain)
            .await?;

        if !execution_result.all_passed {
            return Ok(WorkflowResult {
                todo_id: mapping.todo_id,
                status: WorkflowStatus::Failed,
                subtask_results: execution_result.results,
                message: "部分子任务失败，需要人工介入".to_string(),
            });
        }

        // ===== 阶段 5: 验收 =====
        let acceptance = self
            .acceptance_service
            .accept(mapping.todo_id, &execution_result)
            .await?;

        if !acceptance.passed {
            return Ok(WorkflowResult {
                todo_id: mapping.todo_id,
                status: WorkflowStatus::NeedsRevision,
                subtask_results: execution_result.results,
                message: acceptance.feedback,
            });
        }

        // ===== 阶段 6: 更新文档 =====
        let doc_changes = self
            .documentation_service
            .update_for_completion(mapping.todo_id, &execution_result)
            .await?;

        // ===== 阶段 7: 完成 =====
        self.complete_workflow(mapping.todo_id).await?;

        Ok(WorkflowResult {
            todo_id: mapping.todo_id,
            status: WorkflowStatus::Completed,
            subtask_results: execution_result.results,
            document_changes: doc_changes,
            message: "需求已完成，所有文档已更新".to_string(),
        })
    }

    async fn execute_subtasks(
        &self,
        todo_id: TodoId,
        task_chain: &TaskChain,
    ) -> Result<ExecutionResult, WorkflowError> {
        let mut results = Vec::new();

        for subtask in &task_chain.subtasks {
            // 执行单个子任务
            let result = self
                .step_engine
                .execute_subtask(todo_id, subtask)
                .await?;

            results.push(result);

            // 如果失败，检查是否需要停止
            if !result.passed && result.blocking {
                break;
            }
        }

        Ok(ExecutionResult {
            results,
            all_passed: results.iter().all(|r| r.passed),
        })
    }
}
```

---

## 6. 配置设计

```yaml
# NDC 工程配置
engineering:
  # 阶段 1: 需求理解配置
  understanding:
    # LLM 超时 (秒)
    timeout: 60
    # 最小相似度阈值 (判断是否已有 TODO)
    similarity_threshold: 0.8

  # 阶段 2: TODO 映射配置
  mapping:
    # 是否自动创建 TODO
    auto_create: true
    # 创建后是否通知用户确认
    notify_on_create: true

  # 阶段 3: 任务分解配置
  decomposition:
    # 最大子任务数
    max_subtasks: 20
    # 最小子任务数
    min_subtasks: 1
    # LLM 超时 (秒)
    timeout: 120

  # 阶段 4: 执行配置
  execution:
    # 步骤执行最大重试次数
    max_retries: 3
    #     quality_g质量门禁
ates:
      - "cargo check"
      - "cargo test --lib"
      - "cargo clippy"

  # 阶段 5: 验收配置
  acceptance:
    # 是否需要人工验收
    require_human: false
    # 自动验收阈值
    auto_approve:
      test_coverage_min: 0.8
      all_tests_pass: true

  # 阶段 6: 文档更新配置
  documentation:
    # 自动更新文档
    auto_update: true
    # 需要更新的文档类型
    update_types:
      - "README"
      - "API_DOCS"
      - "CHANGELOG"

  # 阶段 7: 通知配置
  notification:
    # 完成时通知
    notify_on_complete: true
    # 失败时通知
    notify_on_failure: true

# LLM 配置
llm:
  provider: "openai"
  model: "gpt-4o"
  temperature: 0.1

# 知识库配置
knowledge:
  # 文档路径
  paths:
    - "docs/"
    - "README.md"
    - "ARCHITECTURE.md"
  # 排除路径
  exclude:
    - "target/"
    - "*.log"
```

---

## 7. 状态流转总结表

| 阶段 | TodoState | 触发条件 | 后续动作 |
|------|-----------|---------|---------|
| 1 | Pending | 用户输入需求 | 进入 Understanding |
| 2 | Understanding | 需求已理解 | 检查/建立 TODO 映射 |
| 3 | Decomposing | TODO 已映射 | LLM 分解为子任务 |
| 4 | InProgress | 分解完成 | 执行子任务 |
| 5 | AwaitingAcceptance | 子任务完成 | 验收检查 |
| 6 | Documenting | 验收通过 | 更新文档 |
| 7 | Completed | 文档更新完成 | 通知用户 |

---

## 8. 核心优势

1. **知识驱动** - 每个决策都基于知识库
2. **TODO 映射** - 需求可追溯，避免重复
3. **原子分解** - 子任务独立可执行
4. **强制质量** - 质量门禁贯穿始终
5. **文档同步** - 代码变更驱动文档更新
6. **用户闭环** - 完成通知形成闭环
