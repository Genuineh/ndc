# NDC LLM 对接设计文档

> **状态**: LLM-only 架构 - NDC 使用 LLM 进行意图解析，不支持正则表达式回退

## 当前实现

### 意图解析

NDC REPL 使用 LLM Provider 进行智能意图解析：

```rust
// crates/interface/src/repl.rs

/// 使用 LLM 解析用户意图
async fn parse_intent_with_llm(input: &str, state: &ReplState) -> ParsedIntent {
    // TODO: 集成 LLM Provider
    // 当前使用简单的关键词检测作为临时方案
}
```

**特点**:
- 支持 Provider: MiniMax, OpenRouter, OpenAI, Anthropic, Ollama
- 动态模型切换: `/model <provider>/<model>`
- 环境变量使用 `NDC_` 前缀避免冲突
- 无正则表达式依赖

---

## LLM 对接架构

### 设计原则

1. **可插拔的 LLM Provider** - 支持多种 LLM（OpenAI、Anthropic、MiniMax、OpenRouter、Ollama 等）
2. **统一接口** - 抽象 `LlmProvider` trait
3. **动态模型切换** - REPL 支持运行时切换 Provider 和模型
4. **流式输出** - 支持实时显示 LLM 生成内容

### 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         NDC REPL                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐    ┌─────────────────────────────────┐    │
│  │   用户输入      │───▶│  LLM Intent Parser              │    │
│  └─────────────────┘    └─────────────────────────────────┘    │
│                                │                                  │
│                                ▼                                  │
│                    ┌─────────────────────┐                     │
│                    │  LLM Provider       │                     │
│                    │  (MiniMax/OpenRouter│                     │
│                    │   OpenAI/Anthropic/ │                     │
│                    │   Ollama)           │                     │
│                    └─────────────────────┘                     │
│                                │                                │
│                                ▼                               │
│                    ┌─────────────────────┐                     │
│                    │  Intent (结构化)      │                     │
│                    └─────────────────────┘                     │
│                                │                                │
│                                ▼                                │
│                    ┌─────────────────────┐                     │
│                    │  Decision Engine     │                     │
│                    └─────────────────────┘                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 接口设计

### 1. LLM Provider Trait

```rust
// crates/core/src/llm.rs

use async_trait::async_trait;

/// LLM 消息
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// LLM 响应
#[derive(Debug)]
pub struct LlmResponse {
    pub content: String,
    pub usage: Option<LlmUsage>,
    pub model: String,
}

#[derive(Debug)]
pub struct LlmUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// LLM Provider 接口
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 发送消息并获取响应
    async fn chat(&self, messages: &[LlmMessage]) -> Result<LlmResponse, LlmError>;

    /// 发送消息（流式）
    async fn chat_stream(
        &self,
        messages: &[LlmMessage],
    ) -> Result<impl futures::Stream<Item = Result<String, LlmError>>, LlmError>;

    /// 获取模型名称
    fn model_name(&self) -> &str;

    /// 健康检查
    async fn is_healthy(&self) -> bool;
}

/// LLM 错误
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API 错误: {0}")]
    ApiError(String),

    #[error("网络错误: {0}")]
    NetworkError(#[from] std::io::Error),

    #[error("速率限制")]
    RateLimited,

    #[error("模型不支持")]
    ModelNotSupported,

    #[error("认证失败")]
    AuthenticationFailed,

    #[error("上下文超限")]
    ContextLengthExceeded,
}
```

### 2. Intent Parser Trait

```rust
// crates/core/src/intent.rs

use super::{LlmProvider, LlmMessage};

/// 意图类型
#[derive(Debug, Clone, PartialEq)]
pub enum IntentType {
    CreateTask,
    ExecuteTask,
    ListTasks,
    ViewStatus,
    GitOperation,
    FileOperation,
    QueryMemory,
    Unknown,
}

/// 提取的意图
#[derive(Debug, Clone)]
pub struct ParsedIntent {
    pub intent_type: IntentType,
    pub confidence: f64,          // 0.0 - 1.0
    pub entities: HashMap<String, String>,
    pub raw_input: String,
    pub suggested_action: String,
}

/// 意图解析器接口
#[async_trait]
pub trait IntentParser: Send + Sync {
    /// 解析用户输入
    async fn parse(&self, input: &str) -> ParsedIntent;

    /// 批量解析（用于模糊匹配）
    async fn parse_batch(&self, inputs: &[String]) -> Vec<ParsedIntent>;

    /// 设置 LLM provider
    fn set_provider(&mut self, provider: Option<Arc<dyn LlmProvider>>);
}
```

---

## LLM Provider 实现示例

### 1. OpenAI Provider

```rust
// crates/core/src/providers/openai.rs

use async_trait::async_trait;
use reqwest;
use serde::Deserialize;

pub struct OpenAiProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl super::LlmProvider for OpenAiProvider {
    async fn chat(&self, messages: &[LlmMessage]) -> Result<LlmResponse, LlmError> {
        let request_body = serde_json::json!({
            "model": self.model,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                "content": m.content,
            })).collect::<Vec<_>>(),
            "temperature": 0.1,  // 低温度以获得稳定输出
        });

        let response = self.client
            .post(&format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| LlmError::NetworkError(e))?;

        // 处理响应...
        Ok(LlmResponse {
            content: "parsed response".to_string(),
            usage: None,
            model: self.model.clone(),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
```

### 2. Anthropic/Claude Provider

```rust
// crates/core/src/providers/anthropic.rs

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    // ...
}

#[async_trait]
impl super::LlmProvider for AnthropicProvider {
    async fn chat(&self, messages: &[LlmMessage]) -> Result<LlmResponse, LlmError> {
        // Anthropic API 集成
        // 注意: Anthropic 使用不同的消息格式
    }
}
```

### 3. Local/ Ollama Provider

```rust
// crates/core/src/providers/ollama.rs

pub struct OllamaProvider {
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            base_url: "http://localhost:11434".to_string(),
        }
    }
}

#[async_trait]
impl super::LlmProvider for OllamaProvider {
    async fn chat(&self, messages: &[LlmMessage]) -> Result<LlmResponse, LlmError> {
        // Ollama 本地模型集成
        // 使用 /api/chat 接口
    }

    async fn chat_stream(&self, messages: &[LlmMessage]) -> Result<impl Stream<Item=Result<String, LlmError>>, LlmError> {
        // 支持流式输出
    }
}
```

---

## REPL 集成

```rust
// crates/interface/src/repl.rs

use ndc_core::{LlmProvider, IntentParser, ParsedIntent};

pub struct Repl {
    config: ReplConfig,
    state: ReplState,
    parser: Box<dyn IntentParser>,
    llm_provider: Option<Arc<dyn LlmProvider>>,
}

impl Repl {
    pub fn with_llm(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.llm_provider = Some(provider.clone());
        self.parser.set_provider(Some(provider));
        self
    }

    async fn process_input(&mut self, input: &str) {
        // 使用 LLM 解析意图
        let intent = self.parse_intent_with_llm(input).await;
        self.handle_intent(intent).await;
    }

    async fn parse_intent_with_llm(&self, input: &str) -> ParsedIntent {
        // 构建包含对话历史的消息
        let messages = vec![
            Message {
                role: MessageRole::System,
                content: "You are an intent parser...".to_string(),
                name: None,
                tool_calls: None,
            },
            // ... 添加对话历史
            Message {
                role: MessageRole::User,
                content: input.to_string(),
                name: None,
                tool_calls: None,
            },
        ];

        // 调用 LLM Provider
        let response = self.llm_provider.complete(&CompletionRequest {
            model: self.current_model.clone(),
            messages,
            ..Default::default()
        }).await?;

        // 解析响应
        serde_json::from_str(&response.content)?
    }
}
```

---

## 配置

### 环境变量（使用 NDC_ 前缀避免冲突）

```bash
# OpenAI
export NDC_OPENAI_API_KEY="sk-..."
export NDC_OPENAI_MODEL="gpt-4"

# Anthropic
export NDC_ANTHROPIC_API_KEY="sk-ant-..."
export NDC_ANTHROPIC_MODEL="claude-3-opus-20240229"

# MiniMax
export NDC_MINIMAX_API_KEY="your-minimax-api-key"
export NDC_MINIMAX_GROUP_ID="your-group-id"  # 可选
export NDC_MINIMAX_MODEL="m2.1-0107"

# OpenRouter
export NDC_OPENROUTER_API_KEY="your-openrouter-key"
export NDC_OPENROUTER_MODEL="anthropic/claude-3.5-sonnet"
export NDC_OPENROUTER_SITE_URL="https://your-site.com"  # 可选
export NDC_OPENROUTER_APP_NAME="YourAppName"  # 可选

# Ollama (本地)
export NDC_OLLAMA_MODEL="llama2"
export NDC_OLLAMA_URL="http://localhost:11434"
```

### 配置文件

```yaml
# ~/.ndc/config.yaml

llm:
  enabled: true
  provider: "openai"  # openai, anthropic, minimax, openrouter, ollama
  model: "gpt-4"
  temperature: 0.1
  max_tokens: 2048

  # OpenAI 专用
  openai:
    api_key: "${NDC_OPENAI_API_KEY}"

  # Anthropic 专用
  anthropic:
    api_key: "${NDC_ANTHROPIC_API_KEY}"

  # MiniMax 专用
  minimax:
    api_key: "${NDC_MINIMAX_API_KEY}"
    group_id: "${NDC_MINIMAX_GROUP_ID}"

  # OpenRouter 专用
  openrouter:
    api_key: "${NDC_OPENROUTER_API_KEY}"
    site_url: "${NDC_OPENROUTER_SITE_URL}"
    app_name: "${NDC_OPENROUTER_APP_NAME}"
```

---

## MiniMax Provider

MiniMax 是国内领先的 AI 模型提供商，提供高性能的 M2.1 系列模型。

### 支持的模型

| 模型 | 描述 | 上下文长度 |
|------|------|-----------|
| `m2.1-0107` | MiniMax M2.1 主力模型 | 32k |
| `abab6.5s-chat` | 高性能对话模型 | 8k |
| `abab6.5-chat` | 标准对话模型 | 8k |
| `abab5.5-chat` | 上一代对话模型 | 8k |

### 使用示例

```rust
use ndc_core::llm::provider::{MiniMaxProvider, create_minimax_config};
use std::sync::Arc;

// 创建配置
let config = create_minimax_config(
    "your-api-key".to_string(),
    Some("your-group-id".to_string()),  // Group ID (可选)
    Some("m2.1-0107".to_string()),      // 模型 (可选)
);

// 创建 Provider
let token_counter = Arc::new(SimpleTokenCounter::new());
let provider = MiniMaxProvider::new(config, token_counter);

// 或者使用 with_group_id
let provider = MiniMaxProvider::with_group_id(
    config,
    token_counter,
    "your-group-id".to_string(),
);
```

### API 端点

- **基础 URL**: `https://api.minimax.chat/v1`
- **对话接口**: `/text/chatcompletion_v2`
- **模型列表**: `/models`

### 认证方式

MiniMax 使用 Bearer Token 认证，可选的 GroupId 头：

```
Authorization: Bearer {api_key}
GroupId: {group_id}
```

### 获取 API Key

1. 访问 [MiniMax 开放平台](https://api.minimax.chat/)
2. 注册/登录账号
3. 创建应用获取 API Key
4. 获取 Group ID (可选)

---

## OpenRouter Provider

OpenRouter 是一个统一的 LLM API 网关，提供对多个 AI 提供商的访问。

### 支持的提供商

OpenRouter 支持 100+ 模型，包括：

| 提供商 | 模型示例 |
|--------|---------|
| Anthropic | `anthropic/claude-3.5-sonnet`, `claude-3-opus`, `claude-3-haiku` |
| OpenAI | `openai/gpt-4o`, `gpt-4o-mini`, `gpt-4-turbo` |
| Google | `google/gemini-pro-1.5`, `gemini-flash` |
| Meta | `meta-llama/llama-3.1-405b-instruct` |
| Mistral AI | `mistralai/mistral-large`, `mixtral` |

### 使用示例

```rust
use ndc_core::llm::provider::{OpenRouterProvider, create_openrouter_config};
use std::sync::Arc;

// 创建配置
let config = create_openrouter_config(
    "your-openrouter-key".to_string(),
    Some("anthropic/claude-3.5-sonnet".to_string()),  // 模型
    Some("https://your-site.com".to_string()),           // 站点 URL (可选)
    Some("YourAppName".to_string()),                     // 应用名称 (可选)
);

// 创建 Provider
let token_counter = Arc::new(SimpleTokenCounter::new());
let provider = OpenRouterProvider::new(config, token_counter);

// 或者使用 with_site_info
let provider = OpenRouterProvider::with_site_info(
    config,
    token_counter,
    Some("https://your-site.com".to_string()),
    Some("YourAppName".to_string()),
);
```

### 动态获取模型列表

OpenRouter 支持从 API 动态获取可用模型：

```rust
// 获取所有可用模型
let models = provider.list_models().await?;

for model in models {
    println!("Model: {} - Owned by: {}", model.id, model.owned_by);
}
```

### API 端点

- **基础 URL**: `https://openrouter.ai/api/v1`
- **对话接口**: `/chat/completions`
- **模型列表**: `/models`

### 请求头

OpenRouter 使用以下自定义头：

```
Authorization: Bearer {api_key}
HTTP-Referer: {site_url}
X-Title: {app_name}
X-Organization: {organization}  # 可选
```

### 获取 API Key

1. 访问 [OpenRouter.ai](https://openrouter.ai/)
2. 注册/登录账号
3. 获取 API Key
4. 设置应用信息以在排行榜中显示

---

## 使用建议

### 1. 国内环境（推荐 MiniMax）

MiniMax 是国内访问速度最快的选项：

```bash
export NDC_MINIMAX_API_KEY="your-api-key"
export NDC_MINIMAX_GROUP_ID="your-group-id"

ndc repl

# 或在 REPL 中动态切换
/model minimax/m2.1-0107
```

### 2. 多模型访问（推荐 OpenRouter）

OpenRouter 提供统一接口访问多个模型：

```bash
export NDC_OPENROUTER_API_KEY="your-key"

ndc repl

# 在 REPL 中动态切换模型
/model openrouter/anthropic/claude-3.5-sonnet
/model openrouter/openai/gpt-4o
/model openrouter/google/gemini-pro-1.5
```

### 3. 本地开发（Ollama）

完全免费、隐私保护的本地模型：

```bash
# 安装 Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# 下载模型
ollama pull llama2
ollama pull codellama

# 启动
export NDC_OLLAMA_MODEL="codellama"
ndc repl

# 或在 REPL 中
/model ollama/llama3
```

### 4. 混合模式

根据场景选择不同 Provider：

```rust
// 检测 LLM 可用性，自动选择
let provider = detect_best_provider().await;
let repl = Repl::new().with_llm(provider);
```

---

## System Prompt

```text
你是一个专业编程助手，帮助用户完成软件开发任务。

你的工作流程：
1. 理解用户需求，提取关键信息
2. 将需求分解为具体的任务步骤
3. 每个步骤执行前，先获取用户确认
4. 使用工具完成代码编写、测试、文档等任务
5. 完成后向用户报告结果

约束：
- 只执行用户明确授权的操作
- 危险操作（如 rm）需要二次确认
- 保持代码简洁，符合项目规范
- 编写必要的测试用例
```

---

## 使用建议

### 1. 开发环境（本地 Ollama）

```bash
# 安装 Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# 下载模型
ollama pull llama2
ollama pull codellama

# 启动
export NDC_OLLAMA_MODEL="codellama"
ndc repl
```

### 2. 云端（OpenAI/Claude）

```bash
export NDC_OPENAI_API_KEY="sk-..."
ndc repl

# 在 REPL 中切换
/model openai/gpt-4
/model anthropic/claude-3-opus
```

### 3. REPL 动态模型切换

NDC REPL 支持 `/model` 命令动态切换模型：

```bash
ndc repl

# 查看当前模型
> /model

# 切换到 MiniMax
> /model minimax/m2.1-0107

# 切换到 OpenRouter 上的 Claude
> /model openrouter/anthropic/claude-3.5-sonnet

# 切换到本地 Ollama
> /model ollama/llama3
```

### 4. 混合模式

```rust
// 检测 LLM 可用性，自动选择
let provider = detect_best_provider().await;
let repl = Repl::new().with_llm(provider);
```

---

## 下一步

1. **实现 REPL LLM 意图解析** - 在 `parse_intent_with_llm` 中集成 LLM Provider
2. **优化 System Prompt** - 设计意图解析的专用 Prompt
3. **添加流式响应** - 实现实时显示 LLM 生成内容
4. **上下文管理** - 实现对话历史的智能压缩
5. **多轮对话优化** - 改进对话历史的传递方式

---

## 相关资源

- [OpenAI Chat API](https://platform.openai.com/docs/api-reference/chat)
- [Anthropic Messages API](https://docs.anthropic.com/claude/reference/messages)
- [MiniMax API 文档](https://api.minimax.chat/)
- [OpenRouter API 文档](https://openrouter.ai/docs)
- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md)
