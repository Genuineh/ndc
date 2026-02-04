# DevMan Feature Requests

本文档包含待提交给 DevMan 项目的 Feature Request。

---

## Feature Request 1: 向量检索支持知识服务

**标题**: `feat: 向量检索支持知识服务 - 语义搜索能力`

**标签**: `enhancement`, `knowledge`, `vector-search`

**模板**:

```markdown
## 概述

为知识服务添加语义搜索能力，支持基于向量相似度的知识检索。

## 背景

当前知识服务基于关键词搜索，无法理解语义相似性。例如：
- 搜索 "错误处理" 无法匹配 "error handling"
- 搜索 "用户认证" 无法匹配 "authentication"
- 搜索 "如何测试" 无法匹配 "testing guide"

## 建议方案

### 1. 集成向量数据库

推荐使用 Qdrant（轻量、本地运行友好）：

```toml
[dependencies]
qdrant-client = "1.12"
```

### 2. 数据模型扩展

```rust
use devman_core::Knowledge;

pub struct KnowledgeEmbedding {
    pub knowledge_id: String,
    pub embedding: Vec<f32>,        // 1536 维 (OpenAI) 或 768 维 (本地模型)
    pub model: EmbeddingModel,
}

pub enum EmbeddingModel {
    OpenAIAda002,      // OpenAI text-embedding-ada-002
    LocalBGE,          // BAAI/bge-base-en-v1.5 (本地运行)
    LocalMiniLM,       // sentence-transformers/all-MiniLM-L6-v2
}
```

### 3. API 设计

```rust
#[async_trait]
pub trait VectorKnowledgeService: Send + Sync {
    /// 保存知识并生成 embedding
    async fn save_with_embedding(&self, knowledge: Knowledge) -> Result<()>;

    /// 向量相似度搜索
    async fn search_by_vector(
        &self,
        query: &str,
        limit: usize,
        threshold: f32,  // 相似度阈值 (0.0 - 1.0)
    ) -> Result<Vec<Knowledge>>;

    /// 混合搜索（关键词 + 向量）
    async fn search_hybrid(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Knowledge>>;
}
```

### 4. 实现路径

**Phase 1**: MVP（最小可行产品）
- 使用本地 embedding 模型（无需 API key）
- 简单的余弦相似度搜索
- Knowledge 保存时自动生成 embedding

**Phase 2**: 增强功能
- 支持多种 embedding 模型
- 混合搜索（关键词 + 向量加权）
- 结果重排序

**Phase 3**: MCP 集成
- 新增 `devman_search_knowledge_vector` 工具
- 支持流式返回结果

### 5. 配置示例

```toml
# .devman/config.toml
[knowledge.vector]
enabled = true
model = "local-bge"  # or "openai-ada-002"
dimension = 768
threshold = 0.75

[knowledge.vector.openai]
api_key = "sk-..."
api_base = "https://api.openai.com/v1"

[knowledge.vector.local]
model_path = "/path/to/bge-base-en-v1.5"
```

## 优先级

**高** - 语义搜索是 AI 知识服务的核心能力

## 参考实现

- Qdrant Client: https://github.com/qdrant/qdrant-client
- BGE Embeddings: https://huggingface.co/BAAI/bge-base-en-v1.5
- Sentence Transformers: https://github.com/UKPLab/sentence-transformers

## 相关

- Issue: #链接
- 文档: docs/KNOWLEDGE.md
```

---

## Feature Request 2: 知识访问控制

**标题**: `feat: 知识服务访问控制 - 基于角色的权限管理`

**标签**: `enhancement`, `knowledge`, `security`, `access-control`

**模板**:

```markdown
## 概述

为知识服务添加基于角色的访问控制（RBAC），支持细粒度的读写权限管理。

## 背景

当前知识服务没有访问控制机制，在多 Agent 协作场景下存在以下问题：

1. **安全性**: 临时推理可能被错误地当作事实使用
2. **稳定性**: 未验证的知识可能覆盖已验证的内容
3. **可追溯性**: 无法追踪谁创建/修改了知识
4. **协作**: 不同 Agent 角色应有不同的知识访问权限

## 建议方案

### 1. 数据模型扩展

```rust
use std::collections::HashSet;
use devman_core::{AgentId, KnowledgeType};

/// Agent 角色（与 MCP 系统对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRole {
    Planner,      // 规划任务、分解工作
    Implementer,  // 实现代码、执行操作
    Reviewer,     // 审查代码、验证质量
    Tester,       // 运行测试、验证结果
    Historian,    // 记录历史、管理知识
    Admin,        // 管理员
    Any,          // 任何角色（用于公共知识）
}

/// 访问控制列表
#[derive(Debug, Clone)]
pub struct AccessControl {
    pub owner: AgentId,
    pub read_roles: HashSet<AgentRole>,
    pub write_roles: HashSet<AgentRole>,
    pub created_at: DateTime<Utc>,
    pub modified_at: Option<DateTime<Utc>>,
}

impl AccessControl {
    /// 检查读权限
    pub fn can_read(&self, role: &AgentRole) -> bool {
        self.read_roles.contains(role) || self.read_roles.contains(&AgentRole::Any)
    }

    /// 检查写权限
    pub fn can_write(&self, role: &AgentRole) -> bool {
        self.write_roles.contains(role)
    }

    /// 默认访问控制（根据知识类型）
    pub fn default_for(knowledge_type: KnowledgeType, owner: AgentId) -> Self {
        match knowledge_type {
            KnowledgeType::Solution | KnowledgeType::Decision => {
                // 解决方案和决策：所有人可读，只有 Historian/Admin 可写
                Self {
                    owner,
                    read_roles: [AgentRole::Any].into_iter().collect(),
                    write_roles: [AgentRole::Historian, AgentRole::Admin].into_iter().collect(),
                    created_at: Utc::now(),
                    modified_at: None,
                }
            }
            KnowledgeType::CodePattern | KnowledgeType::Template => {
                // 代码模式和模板：Implementer 可写
                Self {
                    owner,
                    read_roles: [AgentRole::Any].into_iter().collect(),
                    write_roles: [AgentRole::Implementer, AgentRole::Historian, AgentRole::Admin]
                        .into_iter().collect(),
                    created_at: Utc::now(),
                    modified_at: None,
                }
            }
            _ => {
                // 默认：所有人可读写
                Self {
                    owner,
                    read_roles: [AgentRole::Any].into_iter().collect(),
                    write_roles: [AgentRole::Any].into_iter().collect(),
                    created_at: Utc::now(),
                    modified_at: None,
                }
            }
        }
    }
}
```

### 2. Knowledge 扩展

```rust
#[derive(Debug, Clone)]
pub struct Knowledge {
    // ... 现有字段 ...

    /// 访问控制
    pub access_control: AccessControl,

    /// 创建者
    pub created_by: AgentId,

    /// 最后修改者
    pub modified_by: Option<AgentId>,
}
```

### 3. API 设计

```rust
#[async_trait]
pub trait AccessControlledKnowledgeService: Send + Sync {
    /// 保存知识（自动应用访问控制）
    async fn save_with_acl(
        &self,
        knowledge: Knowledge,
        requester: AgentId,
        requester_role: AgentRole,
    ) -> Result<Knowledge>;

    /// 搜索知识（只返回有权限访问的）
    async fn search_with_acl(
        &self,
        query: &str,
        requester_role: AgentRole,
        limit: usize,
    ) -> Result<Vec<Knowledge>>;

    /// 更新知识（检查写权限）
    async fn update_with_acl(
        &self,
        id: &str,
        updates: KnowledgeUpdate,
        requester: AgentId,
        requester_role: AgentRole,
    ) -> Result<Knowledge>;

    /// 删除知识（只允许 owner 或 Admin）
    async fn delete_with_acl(
        &self,
        id: &str,
        requester: AgentId,
        requester_role: AgentRole,
    ) -> Result<()>;
}
```

### 4. 权限矩阵

| 知识类型 | Planner | Implementer | Reviewer | Tester | Historian | Admin |
|---------|---------|-------------|----------|--------|-----------|-------|
| Solution | R | R | R | R | RW | RW |
| Decision | R | R | R | R | RW | RW |
| BestPractice | R | R | R | R | RW | RW |
| CodePattern | R | RW | R | R | RW | RW |
| Template | R | RW | R | R | RW | RW |
| LessonLearned | R | R | R | R | RW | RW |

(R = Read, W = Write)

### 5. MCP 集成

```rust
// 新增工具参数
pub struct KnowledgeRequest {
    pub knowledge: Knowledge,
    pub agent_id: String,      // 新增
    pub agent_role: String,    // 新增
}

// 错误响应
pub enum KnowledgeError {
    AccessDenied {
        reason: String,
        required_role: AgentRole,
    },
    // ...
}
```

### 6. 配置示例

```toml
# .devman/config.toml
[knowledge.acl]
enabled = true

[knowledge.acl.defaults]
# 默认访问策略
public_read = true          # 默认所有人可读
restrict_write = false      # 默认不限制写入

# 角色映射（MCP 调用时使用）
[knowledge.acl.role_mapping]
# 可选：将外部角色映射到内部 AgentRole
```

## 优先级

**中** - 多 Agent 协作场景需要，单 Agent 可选

## 安全考虑

1. 所有写操作必须验证 ACL
2. 搜索结果自动过滤无权限访问的知识
3. 审计日志记录所有 ACL 变更
4. Admin 角色可以绕过 ACL（用于紧急修复）

## 相关

- 向量检索 Feature Request
- 记忆稳定性 Feature Request
```

---

## Feature Request 3: 知识稳定性等级

**标题**: `feat: 知识稳定性等级 - 区分临时结论和已验证事实`

**标签**: `enhancement`, `knowledge`, `quality`

**模板**:

```markdown
## 概述

为知识条目添加稳定性等级，区分临时推理结论和已验证的事实，提升知识服务的可信度。

## 背景

当前知识服务没有区分知识的可信度等级，存在以下问题：

1. **推理污染**: AI 推理过程中的临时假设可能被错误保存为"知识"
2. **验证缺失**: 未经过测试或人工确认的内容与已验证内容混在一起
3. **优先级缺失**: 搜索时无法优先返回更可信的内容
4. **演进困难**: 知识从"假设"到"事实"的晋升路径不清晰

## 建议方案

### 1. 稳定性等级定义

```rust
/// 知识稳定性等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum KnowledgeStability {
    /// 临时推理（AI 推理过程中的临时结论，可能被推翻）
    Ephemeral = 0,

    /// 推导结论（通过逻辑推导得出，但未经验证）
    Derived = 1,

    /// 已验证（通过测试或人工确认）
    Verified = 2,

    /// 事实/约束（系统级真理，几乎不会改变）
    Canonical = 3,
}

impl KnowledgeStability {
    /// 获取显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Ephemeral => "临时",
            Self::Derived => "推导",
            Self::Verified => "已验证",
            Self::Canonical => "事实",
        }
    }

    /// 获取置信度分数 (0.0 - 1.0)
    pub fn confidence(&self) -> f32 {
        match self {
            Self::Ephemeral => 0.25,
            Self::Derived => 0.5,
            Self::Verified => 0.75,
            Self::Canonical => 1.0,
        }
    }
}
```

### 2. 数据模型扩展

```rust
#[derive(Debug, Clone)]
pub struct Knowledge {
    // ... 现有字段 ...

    /// 稳定性等级
    pub stability: KnowledgeStability,

    /// 验证记录
    pub verification: Option<VerificationRecord>,
}

/// 验证记录
#[derive(Debug, Clone)]
pub struct VerificationRecord {
    pub verified_at: DateTime<Utc>,
    pub verified_by: AgentId,
    pub verification_method: VerificationMethod,
    pub test_results: Option<String>,
}

/// 验证方法
#[derive(Debug, Clone)]
pub enum VerificationMethod {
    /// 自动测试通过
    AutomatedTest,

    /// 代码审查通过
    CodeReview,

    /// 人工确认
    ManualConfirmation,

    /// 生产环境验证
    ProductionVerified,
}
```

### 3. 晋升机制

```rust
#[async_trait]
pub trait KnowledgePromotion: Send + Sync {
    /// 晋升知识稳定性
    async fn promote(
        &self,
        knowledge_id: &str,
        to: KnowledgeStability,
        reason: &str,
        verifier: AgentId,
    ) -> Result<()>;

    /// 晋升到已验证（需要验证记录）
    async fn promote_to_verified(
        &self,
        knowledge_id: &str,
        verification: VerificationRecord,
    ) -> Result<()>;

    /// 降级（当发现知识错误时）
    async fn demote(
        &self,
        knowledge_id: &str,
        to: KnowledgeStability,
        reason: &str,
    ) -> Result<()>;
}
```

### 4. 稳定性转换规则

| From | To | 条件 | 自动 |
|------|-----|------|------|
| Ephemeral | Derived | 逻辑推导完成 | ✓ |
| Derived | Verified | 测试通过/人工确认 | ✗ |
| Verified | Canonical | 长期稳定 + 多次验证 | ✗ |
| Any | Ephemeral | 发现错误/撤销 | ✗ |

### 5. 搜索排序

```rust
#[async_trait]
pub trait StableKnowledgeService: Send + Sync {
    /// 搜索知识（按稳定性排序）
    async fn search_stable(
        &self,
        query: &str,
        minimum_stability: Option<KnowledgeStability>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        let mut results = self.search(query, limit).await?;

        // 过滤低稳定性
        if let Some(min) = minimum_stability {
            results.retain(|k| k.stability >= min);
        }

        // 按稳定性排序
        results.sort_by(|a, b| b.stability.cmp(&a.stability));

        Ok(results)
    }
}
```

### 6. 默认稳定性

| 知识类型 | 默认稳定性 | 说明 |
|---------|-----------|------|
| LessonLearned | Verified | 经验教训应该是已验证的 |
| BestPractice | Verified | 最佳实践应该是已验证的 |
| Solution | Derived | 解决方案需要验证后才能晋升 |
| Decision | Derived | 决策需要时间验证 |
| CodePattern | Verified | 代码模式应该是已验证的 |
| Template | Derived | 模板可能需要验证 |
| Custom | Ephemeral | 自定义知识默认最低 |

### 7. API 设计

```rust
// 保存知识时指定稳定性
pub struct CreateKnowledgeRequest {
    pub title: String,
    pub knowledge_type: KnowledgeType,
    pub content: String,
    pub stability: KnowledgeStability,  // 新增
    // ...
}

// MCP 工具参数
pub struct KnowledgeWithStability {
    // ... 现有字段 ...
    pub stability: KnowledgeStability,
    pub verification_note: Option<String>,
}
```

### 8. MCP 集成

新增工具：

| 工具名称 | 功能 |
|---------|------|
| `devman_promote_knowledge` | 晋升知识稳定性 |
| `devman_get_knowledge_history` | 获取知识变更历史 |
| `devman_search_verified` | 只搜索已验证的知识 |

### 9. 可视化

在知识条目中显示稳定性：

```markdown
## Rust 错误处理最佳实践

| 属性 | 值 |
|------|-----|
| **稳定性** | ✅ 已验证 |
| **验证方式** | AutomatedTest |
| **验证时间** | 2026-02-03 |
| **置信度** | 75% |

...内容...
```

## 优先级

**中** - 提升知识质量，但非阻塞功能

## 相关

- 访问控制 Feature Request
- 向量检索 Feature Request
```

---

## 提交指南

### 方式 1: GitHub Web UI

1. 访问 https://github.com/Genuineh/DevMan/issues
2. 点击 "New Issue"
3. 选择合适的模板（或使用自定义）
4. 填写上述内容
5. 添加标签：`enhancement`, `knowledge`

### 方式 2: GitHub CLI

```bash
# 安装 gh cli
# https://cli.github.com/

# 提交 Issue 1: 向量检索
gh issue create \
  --repo Genuineh/DevMan \
  --title "feat: 向量检索支持知识服务 - 语义搜索能力" \
  --body-file docs/devman-feature-requests.md \
  --label "enhancement,knowledge,vector-search"

# 提交 Issue 2: 访问控制
gh issue create \
  --repo Genuineh/DevMan \
  --title "feat: 知识服务访问控制 - 基于角色的权限管理" \
  --label "enhancement,knowledge,security,access-control"

# 提交 Issue 3: 稳定性等级
gh issue create \
  --repo Genuineh/DevMan \
  --title "feat: 知识稳定性等级 - 区分临时结论和已验证事实" \
  --label "enhancement,knowledge,quality"
```

### 顺序建议

1. **先提交稳定性等级**（最简单，独立性强）
2. **再提交访问控制**（依赖稳定性）
3. **最后提交向量检索**（复杂度高，需要外部依赖）

---

**生成时间**: 2026-02-04
**项目**: NDC - https://github.com/user/ndc
