# NDC 完整工程约束流程设计

> 整合 Task 状态机、知识库稳定性、质量门禁与 LLM

---

## 1. 系统组件概览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          NDC 完整工程约束流程                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │                    Task 状态机 (TaskState)                          │   │
│   │  Pending → Preparing → InProgress → AwaitingVerification → Done   │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │                  Memory 稳定性层级 (MemoryStability)                │   │
│   │  Ephemeral → Derived → Verified → Canonical                       │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│   ┌───────────────────────────────────────────────────────────────────┐   │
│   │                    质量门禁 (QualityGate)                          │   │
│   │  Test → Lint → TypeCheck → Build → Security                      │   │
│   └───────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 核心设计原则

### 2.1 Memory Stability ↔ Task State 映射

```
┌────────────────────┬─────────────────────┬─────────────────────────────┐
│ Memory Stability    │ Task State          │ 含义                        │
├────────────────────┼─────────────────────┼─────────────────────────────┤
│ Ephemeral           │ Pending             │ LLM 初步分解，待校验         │
│ Ephemeral          │ Preparing           │ 正在分解/规划                │
│ Derived            │ InProgress          │ 任务执行中                   │
│ Verified           │ AwaitingVerification│ 待验证                      │
│ Canonical          │ Completed           │ 已完成 + 验证通过            │
└────────────────────┴─────────────────────┴─────────────────────────────┘
```

### 2.2 状态流转规则

```
                    ┌─────────────────────────────────────┐
                    │           Human Intervention         │
                    │      (人工介入：调整需求/参数)        │
                    └─────────────────┬───────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────────┐
                    │           Task Created              │
                    │      (Memory: Ephemeral)            │
                    └─────────────────┬───────────────────┘
                                      │
                    ┌─────────────────▼───────────────────┐
                    │         Preparing                   │
                    │   LLM 分解需求 → 结构化 TaskPlan   │
                    │      (Memory: Derived)             │
                    └─────────────────┬───────────────────┘
                                      │
                    ┌─────────────────▼───────────────────┐
                    │           Validating               │
                    │   完整性校验 + 依赖校验 + 知识库校验│
                    │         (Memory: Verified)         │
                    └─────────────────┬───────────────────┘
                                      │
                         ┌────────────┴────────────┐
                         │                         │
                         ▼                         ▼
              ┌──────────────────┐      ┌──────────────────┐
              │    Blocked       │      │   InProgress     │
              │  (等待资源/依赖)  │      │   执行步骤       │
              └────────┬─────────┘      └────────┬─────────┘
                       │                         │
                       │                         ▼
                       │              ┌──────────────────┐
                       │              │  Step Complete    │
                       │              │  (质量门禁检查)   │
                       │              └────────┬─────────┘
                       │                         │
                       │              ┌─────────┴─────────┐
                       │              │                   │
                       │              ▼                   ▼
                       │    ┌──────────────────┐ ┌──────────────────┐
                       │    │   Gate Failed     │ │   Gate Passed     │
                       │    │   (重来 N 次)     │ │   (下一步/完成)   │
                       │    └────────┬─────────┘ └────────┬─────────┘
                       │             │                      │
                       │             ▼                      │
                       │    ┌──────────────────┐           │
                       │    │   Human Needed   │           │
                       │    │  (需人工介入)    │           │
                       │    └────────┬─────────┘           │
                       │             │                      │
                       └─────────────┼──────────────────────┘
                                     │
                                     ▼
              ┌─────────────────────────────────────────────┐
              │           AwaitingVerification              │
              │      (Memory: Verified → Canonical)         │
              │      人工复核 / 自动验收                     │
              └─────────────────────┬───────────────────────┘
                                    │
                                    ▼
              ┌─────────────────────────────────────────────┐
              │              Completed                      │
              │      (Memory: Canonical - 稳定知识)         │
              └─────────────────────────────────────────────┘
```

---

## 3. 详细约束规则

### 3.1 Preparing 阶段约束 (LLM 分解)

```rust
// crates/core/src/llm/decomposer.rs

/// 任务分解器 - Preparing 阶段核心组件
pub struct TaskDecomposer {
    llm_provider: Arc<dyn LlmProvider>,
    validator: TaskPlanValidator,
    retry_policy: RetryPolicy,
}

impl TaskDecomposer {
    /// 分解用户需求为结构化任务计划
    pub async fn decompose(
        &self,
        user_request: &str,
        context: &ExecutionContext,
    ) -> Result<ValidatedTaskPlan, DecompositionError> {
        // 1. LLM 生成初始分解
        let initial_plan = self.llm_provider
            .decompose(user_request, context)
            .await?;

        // 2. 强制完整性校验
        let validation_result = self.validator.validate(&initial_plan)?;

        if !validation_result.is_complete() {
            // 3. 不完整 → 重来
            return Err(DecompositionError::Incomplete {
                missing: validation_result.missing_items(),
                attempts: self.retry_policy.current_attempt(),
            });
        }

        // 4. 校验知识库依赖
        let knowledge_check = self.check_knowledge_dependencies(&initial_plan)?;
        if !knowledge_check.all_available() {
            // 知识库缺失 → 标记为 Blocked 或创建获取任务
            return Err(DecompositionError::MissingKnowledge {
                missing: knowledge_check.missing_items(),
            });
        }

        // 5. 返回验证通过的计划
        Ok(ValidatedTaskPlan {
            plan: initial_plan,
            stability: MemoryStability::Derived,
            validated_at: Utc::now(),
        })
    }
}

/// 计划校验器
pub struct TaskPlanValidator;

impl TaskPlanValidator {
    pub fn validate(&self, plan: &TaskPlan) -> ValidationResult {
        ValidationResult {
            has_title: !plan.title.is_empty(),
            has_description: !plan.description.is_empty(),
            has_steps: !plan.steps.is_empty(),
            steps_have_io: plan.steps.iter().all(|s| {
                !s.input.is_empty() && !s.output.is_empty()
            }),
            steps_have_validation: plan.steps.iter().all(|s| {
                s.validation_criteria.is_some()
            }),
            dependencies_resolvable: self.check_dependencies(plan),
        }
    }
}
```

### 3.2 InProgress 阶段约束 (执行与质量门禁)

```rust
// crates/runtime/src/executor/step_engine.rs

/// 步骤执行引擎 - InProgress 阶段核心组件
pub struct StepEngine {
    tool_manager: ToolManager,
    quality_gate: QualityGateRunner,
    retry_engine: RetryEngine,
}

impl StepEngine {
    pub async fn execute_step(
        &self,
        step: &TaskStep,
        context: &ExecutionContext,
    ) -> Result<StepResult, StepError> {
        // 1. 执行前检查
        pre_execution_checks(step)?;

        // 2. 执行步骤 (带重试)
        let result = self.retry_engine
            .execute_with_retry(|| async {
                self.tool_manager.execute(step.action.clone()).await
            })
            .await?;

        // 3. 执行后质量门禁
        let gate_result = self.quality_gate
            .run_for_step(step, &result)
            .await?;

        if !gate_result.passed {
            // 质量门禁失败 → 重来
            return Err(StepError::QualityGateFailed {
                step: step.step_id.clone(),
                errors: gate_result.errors,
                attempt: self.retry_engine.current_attempt(),
            });
        }

        // 4. 更新 Memory 稳定性
        self.update_memory_stability(step, &result)?;

        Ok(result)
    }

    fn update_memory_stability(
        &self,
        step: &TaskStep,
        result: &StepResult,
    ) -> Result<(), StepError> {
        // 根据结果提升 Memory 稳定性
        let new_stability = match result.success {
            true => MemoryStability::Verified,
            false => MemoryStability::Ephemeral,
        };

        // 写入知识库
        self.knowledge_base.store(MemoryEntry {
            content: MemoryContent::from_step(step, result),
            stability: new_stability,
            ..Default::default()
        })?;

        Ok(())
    }
}
```

### 3.3 AwaitingVerification 阶段约束 (验收)

```rust
// crates/runtime/src/verification/verifier.rs

/// 验收验证器 - AwaitingVerification 阶段
pub struct Verifier {
    auto_checks: Vec<AutoCheck>,
    human_review_required: bool,
}

impl Verifier {
    pub async fn verify(&self, task: &Task) -> VerificationResult {
        // 1. 自动验收检查
        let auto_result = self.run_auto_checks(task).await?;

        // 2. 知识库更新 (Ephemeral → Canonical)
        self.knowledge_base.promote_to_canonical(task.id)?;

        // 3. 是否需要人工复核
        if self.human_review_required || !auto_result.sufficient() {
            return VerificationResult::needs_human_review(
                task.id,
                auto_result,
                "人工复核建议：检查业务逻辑正确性".to_string(),
            );
        }

        VerificationResult::approved(task.id)
    }
}

/// 验收标准
struct AcceptanceCriteria {
    /// 测试覆盖率 >= 80%
    test_coverage_min: f64 = 0.8,

    /// 所有测试通过
    all_tests_pass: bool,

    /// 编译无警告
    no_compiler_warnings: bool,

    /// 文档完整
    documentation_complete: bool,

    /// 变更已提交版本控制
    version_controlled: bool,
}
```

---

## 4. 强制重来机制

```rust
// crates/core/src/retry/engine.rs

/// 强制重来引擎 - 核心约束组件
pub struct RetryEngine {
    config: RetryConfig,
    attempt_count: AtomicU32,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,

    /// 重试延迟 (ms)
    pub base_delay_ms: u64,

    /// 指数退避乘数
    pub backoff_multiplier: f64,

    /// 最大延迟 (ms)
    pub max_delay_ms: u64,

    /// 人工介入阈值
    pub human_intervention_threshold: u32,
}

impl RetryEngine {
    pub async fn execute_with_retry<F, T, E>(&self, operation: F) -> Result<T, E>
    where
        F: Fn() -> Pin<Box<dyn Future<Output = Result<T, E>>>>,
        E: RetryableError,
    {
        loop {
            let attempt = self.attempt_count.fetch_add(1, Ordering::SeqCst);

            match operation().await {
                Ok(result) => {
                    self.attempt_count.store(0, Ordering::SeqCst);
                    return Ok(result);
                }
                Err(error) => {
                    if attempt >= self.config.max_attempts - 1 {
                        // 超过重试次数 → 触发人工介入
                        return Err(E::human_intervention_required(
                            error,
                            self.generate_intervention_report(attempt, &error),
                        ));
                    }

                    // 指数退避延迟
                    let delay = self.calculate_delay(attempt);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    fn generate_intervention_report(&self, attempt: u32, error: &impl std::fmt::Debug) -> HumanInterventionReport {
        HumanInterventionReport {
            attempts: attempt,
            error: format!("{:?}", error),
            suggestions: vec![
                "检查输入参数是否正确".to_string(),
                "验证 LLM 分解是否遗漏关键步骤".to_string(),
                "考虑调整需求描述".to_string(),
                "检查依赖资源是否可用".to_string(),
            ],
            required_action: InterventionAction::Any,
        }
    }
}

/// 可重试错误 trait
pub trait RetryableError: std::error::Error {
    fn is_retryable(&self) -> bool;
    fn human_intervention_required(&self, report: HumanInterventionReport) -> Self;
}
```

---

## 5. 完整流程示例

### 示例：实现 REST API 用户认证功能

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ 阶段 1: Preparing (LLM 分解 + 知识库校验)                                      │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  用户输入: "实现用户登录注册功能，包括 JWT 认证和密码加密存储"                   │
│                                                                              │
│  LLM 分解结果:                                                               │
│  ┌─────────────────────────────────────────────────────────────────────┐     │
│  │ TaskPlan {                                                          │     │
│  │   title: "用户认证功能"                                             │     │
│  │   steps: [                                                          │     │
│  │     { id: 1, title: "设计用户表结构", input: "需求文档", ... },     │     │
│  │     { id: 2, title: "实现密码加密模块", input: "算法选择", ... },   │     │
│  │     { id: 3, title: "实现登录接口", input: "API 设计", ... },      │     │
│  │     { id: 4, title: "实现注册接口", input: "API 设计", ... },      │     │
│  │     { id: 5, title: "实现 JWT 认证中间件", input: "JWT 库", ... }, │     │
│  │     { id: 6, title: "编写测试用例", input: "功能列表", ... },      │     │
│  │   ]                                                                 │     │
│  │ }                                                                    │     │
│  └─────────────────────────────────────────────────────────────────────┘     │
│                                                                              │
│  知识库校验:                                                                  │
│  ✓ 用户表结构设计 → 已有参考模式                                              │
│  ✓ 密码加密 → 需要 bcrypt 库（阻塞）                                          │
│  ✓ JWT 认证 → 已有参考实现                                                  │
│                                                                              │
│  结果: Blocked (等待 bcrypt 库集成) / 或自动创建依赖任务                       │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│ 阶段 2: InProgress (执行 + 质量门禁)                                           │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Step 1: 设计用户表结构                                                       │
│  ├─ 执行: 写入 src/models/user.rs                                            │
│  ├─ 质量门禁: cargo check ✓                                                 │
│  ├─ Memory: Verified (已验证)                                               │
│  └─ 结果: 通过 / 重来 (最多 3 次)                                            │
│                                                                              │
│  Step 2: 实现密码加密模块                                                     │
│  ├─ 执行: 使用 bcrypt 加密                                                   │
│  ├─ 质量门禁: cargo test (单元测试) ✓                                        │
│  ├─ Memory: Verified                                                       │
│  └─ 结果: 通过                                                               │
│                                                                              │
│  ... 以此类推 ...                                                            │
│                                                                              │
│  任何步骤质量门禁失败 → 强制重来 N 次 → 人工介入                               │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│ 阶段 3: AwaitingVerification (验收)                                           │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  自动检查:                                                                    │
│  ✓ cargo test --lib --test integration ✓                                     │
│  ✓ cargo clippy ✓                                                           │
│  ✓ cargo build ✓                                                             │
│  ✓ 文档完整 ✓                                                                │
│                                                                              │
│  人工复核: (如配置需要)                                                       │
│  检查业务逻辑正确性                                                            │
│                                                                              │
│  验收通过 → Memory: Canonical                                                 │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. 配置设计

```yaml
# 工程约束配置
engineering:
  # Preparing 阶段配置
  preparing:
    # LLM 分解最大重试次数
    max_decompose_retries: 3
    # 最小步骤数
    min_steps: 1
    # 最大步骤数
    max_steps: 20
    # 知识库缺失时策略
    missing_knowledge_strategy: "block"  # block / auto_fetch / ignore

  # InProgress 阶段配置
  in_progress:
    # 步骤执行最大重试次数
    max_step_retries: 3
    # 质量门禁配置
    quality_gates:
      - "cargo check"
      - "cargo test --lib"
      - "cargo clippy"
    # Memory 升级策略
    memory_upgrade:
      on_success: "verified"
      on_failure: "ephemeral"

  # AwaitingVerification 阶段配置
  awaiting_verification:
    # 是否需要人工复核
    require_human_review: true
    # 自动验收阈值
    auto_approve:
      test_coverage_min: 0.8
      all_tests_pass: true
      no_warnings: true

  # 强制重来配置
  retry:
    max_retries: 3
    base_delay_ms: 1000
    backoff_multiplier: 2
    max_delay_ms: 30000
    human_intervention_after: 3

  # 知识库配置
  knowledge:
    # 是否启用知识库校验
    enable_validation: true
    # 知识库缺失时创建任务
    auto_create_dependency_tasks: true
```

---

## 7. 代码结构

```
crates/core/src/
├── llm/
│   ├── mod.rs
│   ├── decomposer.rs          # 任务分解器
│   ├── validator.rs           # 计划校验器
│   └── retry.rs               # 重试引擎
├── task/
│   ├── mod.rs
│   ├── state_machine.rs       # 状态机
│   └── validators.rs          # 状态转换校验
└── memory/
    ├── mod.rs
    ├── stability.rs           # 稳定性层级
    └── knowledge_base.rs      # 知识库接口

crates/runtime/src/
├── executor/
│   ├── mod.rs
│   ├── step_engine.rs         # 步骤执行引擎
│   └── quality_gate.rs        # 质量门禁
└── verification/
    ├── mod.rs
    └── verifier.rs            # 验收验证器
```

---

## 8. 约束总结表

| 阶段 | 触发条件 | 约束动作 | 失败处理 |
|------|---------|---------|---------|
| **Preparing** | 用户输入 | LLM 分解为 TaskPlan | 重来 N 次 → 人工介入 |
| **Validating** | 分解完成 | 完整性 + 依赖 + 知识库校验 | 重来 → Blocked |
| **InProgress** | 进入执行 | 每个步骤必须通过质量门禁 | 重来 N 次 → 人工介入 |
| **AwaitingVerification** | 步骤完成 | Memory: Verified | 人工/自动验收 |
| **Completed** | 验收通过 | Memory: Canonical | - |
| **Blocked** | 依赖缺失 | 等待资源/人工解决 | 人工介入解决 |
| **Failed** | 重试耗尽 | 任务标记失败 | 人工介入 |

---

## 9. 核心优势

1. **Memory 稳定性驱动开发流程**
   - Ephemeral → Derived → Verified → Canonical
   - 每一步都有明确的质量提升

2. **强制质量门禁**
   - 不通过不让走下一步
   - 代码变更必须经过验证

3. **可追溯性**
   - 每个状态都有 Memory 记录
   - 问题定位清晰

4. **自动化工控流程**
   - 减少人工干预
   - 但保留人工介入能力
