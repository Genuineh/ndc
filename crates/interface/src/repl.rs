//! REPL - 交互式对话模式
//!
//! 职责：
//! - 持续对话
//! - 意图解析（LLM-powered）
//! - 任务自动创建与执行
//! - 上下文保持
//! - AI Agent 模式集成
//!
//! LLM 集成说明：
//! - REPL 通过 LLM Provider 进行意图解析
//! - 使用 /model 命令切换不同的 LLM Provider
//! - 支持的 Provider: MiniMax, OpenRouter, OpenAI, Anthropic, Ollama
//!
//! Agent 模式说明：
//! - 使用 /agent on 启用 AI Agent 模式
//! - AI Agent 可以直接调用工具完成任务
//! - 使用 /agent off 禁用 Agent 模式

use std::path::PathBuf;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ndc_core::{AgentRole, TaskId};
use ndc_runtime::{Executor, tools::ToolRegistry};
use tracing::{info, warn};
use std::collections::HashMap;

// Agent mode integration
use crate::agent_mode::{
    AgentModeManager,
    handle_agent_command,
};

/// REPL 配置
#[derive(Debug, Clone)]
pub struct ReplConfig {
    /// 历史文件
    pub history_file: PathBuf,

    /// 最大历史行数
    pub max_history: usize,

    /// 是否显示思考过程
    pub show_thought: bool,

    /// 提示符
    pub prompt: String,

    /// 自动创建任务
    pub auto_create_task: bool,

    /// 会话超时（秒）
    pub session_timeout: u64,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            history_file: PathBuf::from(".ndc_repl_history"),
            max_history: 1000,
            show_thought: true,
            prompt: "ndc> ".to_string(),
            auto_create_task: true,
            session_timeout: 3600,
        }
    }
}

impl ReplConfig {
    pub fn new(history_file: PathBuf) -> Self {
        Self {
            history_file,
            ..Self::default()
        }
    }
}

/// REPL 状态
#[derive(Debug, Clone)]
pub struct ReplState {
    /// 当前会话ID
    pub session_id: String,

    /// 最后活动时间
    pub last_activity: Instant,

    /// 对话历史
    pub dialogue_history: Vec<DialogueEntry>,

    /// 角色
    pub role: AgentRole,

    /// 创建的任务ID
    pub created_tasks: Vec<TaskId>,

    /// 当前 LLM Provider
    pub current_provider: Option<String>,

    /// 当前 LLM 模型
    pub current_model: Option<String>,
}

impl Default for ReplState {
    fn default() -> Self {
        Self {
            session_id: format!("{:x}", chrono::Utc::now().timestamp_millis()),
            last_activity: Instant::now(),
            dialogue_history: Vec::new(),
            role: AgentRole::Historian,
            created_tasks: Vec::new(),
            current_provider: None,
            current_model: None,
        }
    }
}

impl ReplState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed() > Duration::from_secs(timeout_secs)
    }
}

/// 对话条目
#[derive(Debug, Clone)]
pub struct DialogueEntry {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parsed_intent: Option<ParsedIntent>,
}

/// 解析后的意图
#[derive(Debug, Clone, Default)]
pub struct ParsedIntent {
    pub action_type: ActionType,
    pub target: Option<String>,
    pub description: String,
    pub parameters: HashMap<String, String>,
    pub confidence: f32,
}

/// 动作类型枚举
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ActionType {
    #[default]
    Unknown,
    CreateTask,
    RunTests,
    ReadFile,
    WriteFile,
    CreateFile,
    DeleteFile,
    ListFiles,
    SearchCode,
    GitOperation,
    Refactor,
    Debug,
    Explain,
    Help,
}

/// 运行 REPL
pub async fn run_repl(history_file: PathBuf, executor: Arc<Executor>) {
    let config = ReplConfig::new(history_file);
    let mut state = ReplState::new();

    // 创建 Agent Mode Manager
    let tool_registry = Arc::new(ToolRegistry::new());
    let agent_manager = Arc::new(AgentModeManager::new(executor.clone(), tool_registry));

    info!("Starting NDC REPL with LLM-powered intent parsing and AI Agent support");

    // 打印欢迎信息
    println!(r#"
╔═══════════════════════════════════════════════════════════════════════════════════╗
║  NDC - Neo Development Companion (LLM-Powered REPL + AI Agent)                  ║
║  Features: LLM Intent Parsing | Auto Task Creation | AI Agent Mode             ║
║  Type '/help' for commands, '/agent help' for AI Agent, 'exit' to quit         ║
╚═══════════════════════════════════════════════════════════════════════════════════╝
"#);

    println!("[Session {}] Connected as: {:?} | Model: {:?} @ {:?}",
        state.session_id,
        state.role,
        state.current_model.as_ref().unwrap_or(&"default".to_string()),
        state.current_provider.as_ref().unwrap_or(&"default".to_string())
    );

    // REPL 循环
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        // 检查会话超时
        if state.is_expired(config.session_timeout) {
            println!("\n[Session expired after {}s of inactivity]", config.session_timeout);
            println!("Type 'exit' to quit or 'new' to start a new session.");
        }

        // 根据Agent状态调整提示符
        let prompt = if agent_manager.is_enabled().await {
            "ndc[agent]> ".to_string()
        } else {
            config.prompt.clone()
        };

        print!("{}", prompt);
        io::stdout().flush().unwrap();

        input.clear();
        state.last_activity = Instant::now();

        match stdin.lock().read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                // 处理命令或对话
                if input.starts_with('/') {
                    // 检查是否是 /agent 命令
                    if input.starts_with("/agent") {
                        if let Err(e) = handle_agent_command(input, &agent_manager).await {
                            println!("[Agent Error] {}", e);
                        }
                    } else {
                        handle_command(input, &config, &mut state, executor.clone()).await;
                    }
                } else {
                    // 检查 Agent 模式是否启用
                    if agent_manager.is_enabled().await {
                        // Agent 模式 - 由 AI 处理
                        handle_agent_dialogue(input, &agent_manager).await;
                    } else {
                        // 普通 REPL 模式 - 意图解析
                        handle_dialogue(input, &config, &mut state, executor.clone()).await;
                    }
                }
            }
            Err(e) => {
                warn!("Read error: {}", e);
                break;
            }
        }
    }

    info!("REPL exited");
}

// ===== 命令处理 =====

async fn handle_command(input: &str, config: &ReplConfig, state: &mut ReplState, executor: Arc<Executor>) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "/help" | "/h" => show_help(),
        "/exit" | "/quit" | "/q" => {
            println!("Goodbye! Session: {}", state.session_id);
            std::process::exit(0);
        }
        "/status" | "/st" => show_status(state, config).await,
        "/tasks" => list_tasks(state, &executor).await,
        "/switch" if parts.len() > 1 => switch_role(parts[1], state),
        "/verbose" | "/v" => {
            println!("[Verbose mode: {}]", if config.show_thought { "ON" } else { "OFF" });
        }
        "/clear" | "/cls" => clear_screen(),
        "/new" => {
            state.session_id = format!("{:x}", chrono::Utc::now().timestamp_millis());
            state.dialogue_history.clear();
            state.created_tasks.clear();
            state.current_provider = None;
            state.current_model = None;
            println!("[New session started: {}]", state.session_id);
        }
        "/context" | "/ctx" => show_context(state),
        "/model" | "/m" => {
            if parts.len() > 1 {
                switch_model(parts[1], state);
            } else {
                show_model_info(state);
            }
        }
        "/create" if parts.len() > 1 => {
            let task_title = &input[8..].trim().trim_matches('"');
            create_task_from_input(task_title, state, executor).await;
        }
        _ => println!("Unknown command: {}. Type '/help' for available commands.", cmd),
    }
}

// ===== 对话处理 =====

async fn handle_dialogue(input: &str, config: &ReplConfig, state: &mut ReplState, executor: Arc<Executor>) {
    // 记录对话
    let entry = DialogueEntry {
        role: "user".to_string(),
        content: input.to_string(),
        timestamp: chrono::Utc::now(),
        parsed_intent: None,
    };
    state.dialogue_history.push(entry.clone());

    // LLM 意图解析（待实现）
    let parsed = parse_intent_with_llm(input, state).await;

    if config.show_thought {
        print_thought_process(&parsed);
    }

    // 更新条目的解析结果
    if let Some(last) = state.dialogue_history.last_mut() {
        last.parsed_intent = Some(parsed.clone());
    }

    // 根据意图类型处理
    match parsed.action_type {
        ActionType::Help => show_help(),
        ActionType::CreateTask => {
            create_task_from_input(&parsed.description, state, executor).await;
        }
        ActionType::Unknown => {
            println!("[LLM Analysis] Understanding: {}", parsed.description);
            if config.auto_create_task {
                println!("[Auto-creating task based on your input...]");
                create_task_from_input(input, state, executor).await;
            } else {
                println!("Use '/create \"task title\"' to create a task explicitly.");
            }
        }
        _ => {
            println!("[{:?}] {}", state.role, parsed.description);
            if config.auto_create_task {
                create_task_from_input(&parsed.description, state, executor).await;
            }
        }
    }
}

// ===== Agent 对话处理 =====

/// 处理 Agent 模式下的用户输入
async fn handle_agent_dialogue(input: &str, agent_manager: &Arc<AgentModeManager>) {
    println!("\n🤖 Processing...");

    match agent_manager.process_input(input).await {
        Ok(response) => {
            println!("\n{}\n", response.content);

            if !response.tool_calls.is_empty() {
                let tool_names: Vec<&str> = response.tool_calls.iter()
                    .map(|t| t.name.as_str())
                    .collect();
                println!("🔧 Tools used: {}", tool_names.join(", "));
            }

            if response.is_complete {
                println!("✅ Task completed!");
            }

            if let Some(verification) = response.verification_result {
                match verification {
                    ndc_core::VerificationResult::Completed => {
                        println!("✅ Verification passed!");
                    }
                    ndc_core::VerificationResult::Incomplete { reason } => {
                        println!("⚠️  Incomplete: {}", reason);
                    }
                    ndc_core::VerificationResult::QualityGateFailed { reason } => {
                        println!("❌ Quality gate failed: {}", reason);
                    }
                }
            }
        }
        Err(e) => {
            println!("\n❌ Agent Error: {}", e);
        }
    }
}

// ===== LLM 意图解析 =====

/// 使用 LLM 解析用户意图
///
/// TODO: 集成 LLM Provider 进行意图解析
/// 1. 构建包含对话历史的 prompt
/// 2. 调用当前配置的 LLM Provider
/// 3. 解析 LLM 返回的 JSON 格式意图
async fn parse_intent_with_llm(input: &str, _state: &ReplState) -> ParsedIntent {
    // TODO: 替换为实际的 LLM 调用
    //
    // 示例实现：
    // ```rust
    // use ndc_core::llm::LlmProvider;
    //
    // let messages = vec![
    //     Message {
    //         role: MessageRole::System,
    //         content: "You are an intent parser. Return JSON with action_type, description, etc.".to_string(),
    //         name: None,
    //         tool_calls: None,
    //     },
    //     Message {
    //         role: MessageRole::User,
    //         content: input.to_string(),
    //         name: None,
    //         tool_calls: None,
    //     },
    // ];
    //
    // let response = llm_provider.complete(&CompletionRequest { ... }).await?;
    // ```

    // 临时实现：返回基本意图
    let lower = input.to_lowercase();
    let mut parsed = ParsedIntent::default();
    parsed.description = input.to_string();

    // 简单的关键词检测（临时方案，将被 LLM 替代）
    if lower.contains("create task") || lower.contains("new task") || lower.contains("create a") {
        parsed.action_type = ActionType::CreateTask;
        parsed.confidence = 0.7;
    } else if lower.contains("help") || lower.contains("what can you do") {
        parsed.action_type = ActionType::Help;
        parsed.confidence = 0.9;
    } else if lower.contains("test") {
        parsed.action_type = ActionType::RunTests;
        parsed.confidence = 0.6;
    } else if lower.contains("explain") || lower.contains("how does") {
        parsed.action_type = ActionType::Explain;
        parsed.confidence = 0.7;
    } else {
        parsed.action_type = ActionType::Unknown;
        parsed.confidence = 0.3;
    }

    parsed
}

// ===== 任务创建 =====

async fn create_task_from_input(input: &str, state: &mut ReplState, executor: Arc<Executor>) {
    let title: String = {
        // 简单提取：取前 10 个单词作为标题
        let words: Vec<&str> = input.split_whitespace().take(10).collect();
        words.join(" ")
    };

    println!("[Creating Task] Title: \"{}\"", title);

    match executor.create_task(
        title.clone(),
        format!("Auto-created from REPL session: {}", input),
        state.role,
    ).await {
        Ok(task) => {
            println!("[Task Created] ID: {}", task.id);
            state.created_tasks.push(task.id);
            println!("  Status: {:?}", task.state);
            println!("  Created by: {:?}", task.metadata.created_by);
        }
        Err(e) => {
            println!("[Error] Failed to create task: {}", e);
        }
    }
}

// ===== 辅助函数 =====

fn print_thought_process(parsed: &ParsedIntent) {
    println!("[LLM Thinking...]");
    println!("  Action: {:?}", parsed.action_type);
    println!("  Confidence: {:.0}%", parsed.confidence * 100.0);

    if !parsed.parameters.is_empty() {
        println!("  Params: {:?}", parsed.parameters);
    }

    if let Some(ref target) = parsed.target {
        println!("  Target: {}", target);
    }
}

fn show_help() {
    println!(r#"
Available Commands:
  /help, /h          Show this help
  /status, /st       Show current session status
  /tasks             List all tasks
  /switch <role>     Switch agent role
  /model, /m         Show or switch LLM model (e.g., /model minimax/m2.1-0107)
  /verbose, /v       Toggle thought display
  /clear, /cls       Clear screen
  /new               Start new session
  /context, /ctx     Show context
  /exit, /quit, /q  Exit REPL

LLM Configuration:
  Use /model to switch between LLM providers
  Supported: minimax, openrouter, openai, anthropic, ollama
  Environment: NDC_MINIMAX_API_KEY, NDC_OPENROUTER_API_KEY, etc.

Natural Language Examples:
  "Create a task to add user authentication"
  "Run tests for the API"
  "Fix the error in database.rs"
  "Explain how the executor works"
  "What files were changed?"
"#);
}

async fn show_status(state: &ReplState, config: &ReplConfig) {
    println!("Session Status:");
    println!("  Session ID: {}", state.session_id);
    println!("  Role: {:?}", state.role);
    println!("  Provider: {}", state.current_provider.as_ref().unwrap_or(&"default".to_string()));
    println!("  Model: {}", state.current_model.as_ref().unwrap_or(&"default".to_string()));
    println!("  Auto-create tasks: {}", config.auto_create_task);
    println!("  Dialogue entries: {}", state.dialogue_history.len());
    println!("  Created tasks: {}", state.created_tasks.len());
}

async fn list_tasks(state: &ReplState, executor: &Executor) {
    println!("Tasks:");

    if state.created_tasks.is_empty() {
        println!("  No tasks created in this session");
    } else {
        println!("  Created in this session ({}):", state.created_tasks.len());
        for task_id in &state.created_tasks {
            println!("    - {}", task_id);
        }
    }

    match executor.context().storage.list_tasks().await {
        Ok(tasks) => {
            println!("  All tasks ({}):", tasks.len());
            for task in tasks.iter().take(5) {
                println!("    - {} [{:?}] {}", task.id, task.state, task.title);
            }
            if tasks.len() > 5 {
                println!("    ... and {} more", tasks.len() - 5);
            }
        }
        Err(e) => println!("  [Error listing tasks: {}]", e),
    }
}

fn switch_role(role_str: &str, state: &mut ReplState) {
    match role_str.to_lowercase().as_str() {
        "planner" => state.role = AgentRole::Planner,
        "implementer" => state.role = AgentRole::Implementer,
        "reviewer" => state.role = AgentRole::Reviewer,
        "tester" => state.role = AgentRole::Tester,
        "historian" => state.role = AgentRole::Historian,
        _ => {
            println!("Unknown role: {}. Available: planner, implementer, reviewer, tester, historian", role_str);
            return;
        }
    }

    println!("[Switched to {:?}]", state.role);
}

fn show_context(state: &ReplState) {
    println!("Current Context:");
    println!("  Session: {}", state.session_id);
    println!("  Role: {:?}", state.role);
    println!("  Provider: {:?}", state.current_provider);
    println!("  Model: {:?}", state.current_model);
    println!("  Tasks created: {}", state.created_tasks.len());
}

fn clear_screen() {
    print!("\x1B[2J\x1B[3J\x1B[H");
    let _ = io::stdout().flush();
}

fn show_model_info(state: &ReplState) {
    println!("Current Model Configuration:");
    println!("  Provider: {}", state.current_provider.as_ref().unwrap_or(&"default".to_string()));
    println!("  Model: {}", state.current_model.as_ref().unwrap_or(&"default".to_string()));
    println!();
    println!("Available providers: openai, anthropic, minimax, openrouter, ollama");
    println!();
    println!("Usage: /model <provider>[/<model>]");
    println!();
    println!("Examples:");
    println!("  /model minimax");
    println!("  /model minimax/m2.1-0107");
    println!("  /model openrouter");
    println!("  /model openrouter/anthropic/claude-3.5-sonnet");
    println!("  /model openai/gpt-4o");
    println!();
    println!("Environment Variables (with NDC_ prefix):");
    println!("  NDC_OPENAI_API_KEY, NDC_OPENAI_MODEL");
    println!("  NDC_ANTHROPIC_API_KEY, NDC_ANTHROPIC_MODEL");
    println!("  NDC_MINIMAX_API_KEY, NDC_MINIMAX_GROUP_ID, NDC_MINIMAX_MODEL");
    println!("  NDC_OPENROUTER_API_KEY, NDC_OPENROUTER_MODEL");
    println!("  NDC_OLLAMA_MODEL, NDC_OLLAMA_URL");
}

fn switch_model(model_spec: &str, state: &mut ReplState) {
    let parts: Vec<&str> = model_spec.split('/').collect();

    match parts.first() {
        Some(&"minimax") => {
            state.current_provider = Some("minimax".to_string());
            state.current_model = parts.get(1)
                .map(|s| s.to_string())
                .or_else(|| Some("m2.1-0107".to_string()));
            println!("[Switched to MiniMax: {}]", state.current_model.as_ref().unwrap());
        }
        Some(&"openrouter") => {
            state.current_provider = Some("openrouter".to_string());
            state.current_model = parts.get(1)
                .map(|s| s.to_string())
                .or_else(|| Some("anthropic/claude-3.5-sonnet".to_string()));
            println!("[Switched to OpenRouter: {}]", state.current_model.as_ref().unwrap());
        }
        Some(&"openai") => {
            state.current_provider = Some("openai".to_string());
            state.current_model = parts.get(1)
                .map(|s| s.to_string())
                .or_else(|| Some("gpt-4o".to_string()));
            println!("[Switched to OpenAI: {}]", state.current_model.as_ref().unwrap());
        }
        Some(&"anthropic") => {
            state.current_provider = Some("anthropic".to_string());
            state.current_model = parts.get(1)
                .map(|s| s.to_string())
                .or_else(|| Some("claude-3-opus".to_string()));
            println!("[Switched to Anthropic: {}]", state.current_model.as_ref().unwrap());
        }
        Some(&"ollama") => {
            state.current_provider = Some("ollama".to_string());
            state.current_model = parts.get(1)
                .map(|s| s.to_string())
                .or_else(|| Some("llama3".to_string()));
            println!("[Switched to Ollama: {}]", state.current_model.as_ref().unwrap());
        }
        Some(provider) => {
            state.current_provider = Some(provider.to_string());
            state.current_model = parts.get(1).map(|s| s.to_string());
            println!("[Switched to {}: {:?}]", provider, state.current_model);
        }
        None => {
            println!("Usage: /model <provider>[/<model>]");
            println!("Run /model to see available providers.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_state_default() {
        let state = ReplState::default();
        assert!(!state.session_id.is_empty());
        assert_eq!(state.role, AgentRole::Historian);
        assert!(state.dialogue_history.is_empty());
    }

    #[test]
    fn test_repl_state_not_expired() {
        let state = ReplState::default();
        assert!(!state.is_expired(3600));
        assert!(state.is_expired(0));
    }

    #[test]
    fn test_parsed_intent_default() {
        let parsed = ParsedIntent::default();
        assert_eq!(parsed.action_type, ActionType::Unknown);
        assert_eq!(parsed.confidence, 0.0);
    }

    #[test]
    fn test_repl_config_default() {
        let config = ReplConfig::default();
        assert_eq!(config.max_history, 1000);
        assert!(config.show_thought);
        assert!(config.auto_create_task);
        assert_eq!(config.session_timeout, 3600);
    }

    #[test]
    fn test_dialogue_entry() {
        let entry = DialogueEntry {
            role: "user".to_string(),
            content: "test input".to_string(),
            timestamp: chrono::Utc::now(),
            parsed_intent: None,
        };
        assert_eq!(entry.role, "user");
        assert_eq!(entry.content, "test input");
    }

    #[test]
    fn test_action_type_variants() {
        let create = ActionType::CreateTask;
        let test = ActionType::RunTests;
        let unknown = ActionType::Unknown;

        assert_eq!(create, ActionType::CreateTask);
        assert_eq!(test, ActionType::RunTests);
        assert_eq!(unknown, ActionType::Unknown);
    }
}
