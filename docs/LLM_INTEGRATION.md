# NDC LLM 对接设计文档

> **状态**: 待实现 - NDC 预留了 LLM 接口，当前使用正则表达式进行意图解析

## 当前实现

### 意图解析（当前）

NDC REPL 目前使用正则表达式进行简单的意图识别：

```rust
// crates/interface/src/repl.rs

// 示例：检测创建任务意图
let create_pattern = Regex::new(r"(?i)(create|新建|创建)\s+(.+)").unwrap();

// 示例：检测执行意图
let run_pattern = Regex::new(r"(?i)(run|执行|运行)\s+(.+)").unwrap();
```

**优点**: 快速、无延迟、无需 API Key
**缺点**: 只能匹配预定义模式，无法理解自然语言

---

## LLM 对接架构

### 设计原则

1. **可插拔的 LLM Provider** - 支持多种 LLM（OpenAI、Anthropic、Claude、Local 等）
2. **统一接口** - 抽象 `LlmProvider` trait
3. **降级策略** - LLM 不可用时自动回退到正则匹配
4. **流式输出** - 支持实时显示 LLM 生成内容

### 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         NDC REPL                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐    ┌─────────────────────────────────┐    │
│  │   用户输入      │───▶│  Intent Parser (意图解析器)      │    │
│  └─────────────────┘    └─────────────────────────────────┘    │
│                                │                                  │
│                 ┌──────────────┼──────────────┐                 │
│                 ▼              ▼              ▼                 │
│         ┌───────────┐  ┌───────────┐  ┌───────────┐           │
│         │  Regex    │  │   LLM     │  │  Fallback │           │
│         │  Parser   │  │  Parser   │  │  Parser   │           │
│         └───────────┘  └───────────┘  └───────────┘           │
│                 │              │              │                 │
│                 └──────────────┼──────────────┘                 │
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
        if let Some(ref provider) = self.llm_provider {
            if provider.is_healthy().await {
                let intent = self.parser.parse(input).await;
                self.handle_intent(intent).await;
                return;
            }
        }

        // 降级到正则匹配
        let intent = self.parse_with_regex(input);
        self.handle_intent(intent).await;
    }
}
```

---

## 配置

### 环境变量

```bash
# OpenAI
export OPENAI_API_KEY="sk-..."
export OPENAI_MODEL="gpt-4"

# Anthropic
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_MODEL="claude-3-opus-20240229"

# Ollama (本地)
export OLLAMA_MODEL="llama2"
export OLLAMA_URL="http://localhost:11434"
```

### 配置文件

```yaml
# ~/.ndc/config.yaml

llm:
  enabled: true
  provider: "openai"  # openai, anthropic, ollama, none
  model: "gpt-4"
  temperature: 0.1
  max_tokens: 2048
  fallback_to_regex: true

  # OpenAI 专用
  openai:
    api_key: "${OPENAI_API_KEY}"

  # Anthropic 专用
  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
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
export OLLAMA_MODEL="codellama"
ndc repl
```

### 2. 云端（OpenAI/Claude）

```bash
export OPENAI_API_KEY="sk-..."
ndc repl --llm openai --model gpt-4
```

### 3. 混合模式

```rust
// 检测 LLM 可用性，自动选择
let provider = detect_best_provider().await;
let repl = Repl::new().with_llm(provider);
```

---

## 下一步

1. **实现 LlmProvider trait** - 选择一个 Provider 开始
2. **实现 IntentParser** - 使用 LLM 进行自然语言理解
3. **添加配置支持** - 环境变量和配置文件
4. **实现降级策略** - LLM 不可用时回退
5. **优化 Prompt** - 根据使用场景调整

---

## 相关资源

- [OpenAI Chat API](https://platform.openai.com/docs/api-reference/chat)
- [Anthropic Messages API](https://docs.anthropic.com/claude/reference/messages)
- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md)
- [Claude Code SDK](https://docs.anthropic.com/claude-code/home)
