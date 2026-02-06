//! REPL - 交互式对话模式
//!
//! 职责：
//! - 持续对话
//! - 意图解析（LLM-powered，纯 LLM，无正则 fallback）
//! - 任务自动创建与执行
//! - 上下文保持
//!
//! ⚠️ 注意：当前使用正则作为临时实现，LLM 集成后将移除

use std::path::PathBuf;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use ndc_core::{AgentRole, TaskId};
use ndc_runtime::{Executor};
use tracing::{info, warn};
// TODO: LLM 集成后移除 regex 依赖
use regex::Regex;
use std::collections::HashMap;

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

    /// 提取的实体
    pub entities: ExtractedEntities,

    /// 角色
    pub role: AgentRole,

    /// 创建的任务ID
    pub created_tasks: Vec<TaskId>,

    /// 任务建议
    pub task_suggestions: Vec<TaskSuggestion>,
}

impl Default for ReplState {
    fn default() -> Self {
        Self {
            session_id: format!("{:x}", chrono::Utc::now().timestamp_millis()),
            last_activity: Instant::now(),
            dialogue_history: Vec::new(),
            entities: ExtractedEntities::default(),
            role: AgentRole::Historian,
            created_tasks: Vec::new(),
            task_suggestions: Vec::new(),
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

/// 提取的实体
#[derive(Debug, Clone, Default)]
pub struct ExtractedEntities {
    pub file_paths: Vec<String>,
    pub functions: Vec<String>,
    pub task_names: Vec<String>,
    pub error_messages: Vec<String>,
    pub code_snippets: Vec<String>,
}

/// 任务建议
#[derive(Debug, Clone)]
pub struct TaskSuggestion {
    pub title: String,
    pub description: String,
    pub priority: String,
    pub confidence: f32,
}

/// 运行 REPL
pub async fn run_repl(history_file: PathBuf, executor: Arc<Executor>) {
    let config = ReplConfig::new(history_file);
    let mut state = ReplState::new();

    info!("Starting NDC REPL with full context support");

    // 打印欢迎信息
    println!(r#"
╔═══════════════════════════════════════════════════════════════════════════════════╗
║  NDC - Neo Development Companion (Enhanced REPL)                              ║
║  Features: Intent Parsing | Auto Task Creation | Context Persistence          ║
║  Type 'help' for commands, 'exit' to quit                                   ║
╚═══════════════════════════════════════════════════════════════════════════════════╝
"#);

    println!("[Session {}] Connected as: {:?}", state.session_id, state.role);

    // REPL 循环
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        // 检查会话超时
        if state.is_expired(config.session_timeout) {
            println!("\n[Session expired after {}s of inactivity]", config.session_timeout);
            println!("Type 'exit' to quit or 'new' to start a new session.");
        }

        print!("{}", config.prompt);
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
                    handle_command(input, &config, &mut state, executor.clone()).await;
                } else {
                    handle_dialogue(input, &config, &mut state, executor.clone()).await;
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
        "/help" => show_help(),
        "/exit" | "/quit" | "/q" => {
            println!("Goodbye! Session: {}", state.session_id);
            std::process::exit(0);
        }
        "/status" => show_status(state, config).await,
        "/tasks" => list_tasks(state, &executor).await,
        "/switch" if parts.len() > 1 => switch_role(parts[1], state),
        "/verbose" | "/v" => {
            println!("[Verbose mode: {}]", if config.show_thought { "ON" } else { "OFF" });
        }
        "/clear" | "/cls" => clear_screen(),
        "/new" => {
            state.session_id = format!("{:x}", chrono::Utc::now().timestamp_millis());
            state.dialogue_history.clear();
            state.entities = ExtractedEntities::default();
            state.created_tasks.clear();
            state.task_suggestions.clear();
            println!("[New session started: {}]", state.session_id);
        }
        "/context" | "/ctx" => show_context(state),
        "/create" if parts.len() > 1 => {
            let task_title = &input[8..].trim().trim_matches('"');
            create_task_from_input(task_title, state, executor).await;
        }
        "/suggest" | "/suggests" => show_suggestions(state),
        _ => println!("Unknown command: {}", cmd),
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

    // 解析意图
    let parsed = parse_intent(input, state);

    if config.show_thought {
        print_thought_process(&parsed, state);
    }

    // 更新条目的解析结果
    if let Some(last) = state.dialogue_history.last_mut() {
        last.parsed_intent = Some(parsed.clone());
    }

    // 根据意图类型处理
    match parsed.action_type {
        ActionType::Help => show_help(),
        ActionType::CreateTask => {
            create_task_from_input(input, state, executor).await;
        }
        ActionType::RunTests => {
            println!("[Action] Running tests... (placeholder)");
        }
        ActionType::Explain => {
            println!("[Action] Providing explanation...");
        }
        ActionType::Unknown => {
            // 生成任务建议
            generate_task_suggestions(input, state);
            if !state.task_suggestions.is_empty() {
                println!("[Suggestion] I can help you with:");
                for (i, suggestion) in state.task_suggestions.iter().take(3).enumerate() {
                    println!("  {}. {} ({:.0}%)", i + 1, suggestion.title, suggestion.confidence * 100.0);
                }
                println!("  Use '/create \"task title\"' to create a task");
            }
        }
        _ => {
            println!("[{:?}] I understand you want to: {}", state.role, parsed.description);
            if config.auto_create_task {
                create_task_from_input(input, state, executor).await;
            }
        }
    }
}

// ===== 意图解析 =====

fn parse_intent(input: &str, state: &ReplState) -> ParsedIntent {
    let lower = input.to_lowercase();
    let mut parsed = ParsedIntent::default();

    // 提取任务名称
    let mut entities = state.entities.clone();
    parse_task_names(input, &mut parsed, &mut entities);

    // 检测动作类型
    detect_action_type(&lower, &mut parsed);

    // 提取文件路径
    parse_file_paths(input, &mut entities);

    // 提取错误信息
    parse_error_messages(input, &mut entities);

    // 提取代码片段
    parse_code_snippets(input, &mut entities);

    // 提取函数名
    parse_functions(input, &mut entities);

    // 设置描述
    parsed.description = input.to_string();
    parsed.parameters = extract_parameters(input);

    parsed
}

fn detect_action_type(lower: &str, parsed: &mut ParsedIntent) {
    let patterns = [
        (ActionType::CreateTask, vec!["create task", "new task", "add task", "create a", "i want to create", "make a task"]),
        (ActionType::RunTests, vec!["run test", "execute test", "test the", "run tests", "testing"]),
        (ActionType::ReadFile, vec!["read file", "show file", "view file", "cat file", "open file", "display file"]),
        (ActionType::WriteFile, vec!["write file", "edit file", "modify file", "update file", "change file"]),
        (ActionType::CreateFile, vec!["create file", "new file", "add file", "make a file"]),
        (ActionType::DeleteFile, vec!["delete file", "remove file", "drop file"]),
        (ActionType::ListFiles, vec!["list files", "ls", "dir", "show files", "list directory"]),
        (ActionType::SearchCode, vec!["search", "find", "grep", "look for", "search for"]),
        (ActionType::GitOperation, vec!["git commit", "git push", "git pull", "git status", "git branch", "git checkout"]),
        (ActionType::Refactor, vec!["refactor", "rename", "restructure", "reorganize"]),
        (ActionType::Debug, vec!["debug", "fix error", "fix bug", "fix the error", "solve error", "troubleshoot"]),
        (ActionType::Explain, vec!["explain", "what is", "how does", "tell me about", "describe"]),
        (ActionType::Help, vec!["help", "what can you do", "commands", "how to use"]),
    ];

    for (action_type, keywords) in patterns {
        for keyword in keywords {
            if lower.contains(keyword) {
                parsed.action_type = action_type;
                parsed.confidence = calculate_confidence(keyword, lower);
                return;
            }
        }
    }

    parsed.confidence = 0.3; // 默认低置信度
}

fn calculate_confidence(keyword: &str, text: &str) -> f32 {
    let keyword_len = keyword.len() as f32;
    let text_len = text.len() as f32;
    let ratio = keyword_len / text_len;

    // 越短越精确
    let base = if ratio > 0.1 { 0.9 } else { 0.7 };
    (base + ratio).min(1.0)
}

fn parse_task_names(input: &str, parsed: &mut ParsedIntent, entities: &mut ExtractedEntities) {
    // 匹配引号中的任务名
    let re = Regex::new(r#"["']([^"']+)["']"#).unwrap();
    for cap in re.captures_iter(input) {
        if let Some(name) = cap.get(1) {
            let name = name.as_str().trim().to_string();
            if !name.is_empty() && name.len() > 3 {
                entities.task_names.push(name.clone());
                if parsed.target.is_none() {
                    parsed.target = Some(name);
                }
            }
        }
    }
}

fn parse_file_paths(input: &str, entities: &mut ExtractedEntities) {
    // 匹配常见的文件路径模式
    let patterns = [
        Regex::new(r"[./~\w/-]+\.(rs|toml|md|json|yaml|yml|txt|sh|py|js|ts|html|css)").unwrap(),
        Regex::new(r"(?:src/|crates/|bin/|tests?/|docs?/)[\w/-]+").unwrap(),
    ];

    for re in &patterns {
        for cap in re.find_iter(input) {
            let path = cap.as_str().to_string();
            if !entities.file_paths.contains(&path) {
                entities.file_paths.push(path);
            }
        }
    }
}

fn parse_error_messages(input: &str, entities: &mut ExtractedEntities) {
    // 匹配错误信息
    let re = Regex::new(r"(?:error|exception|failure|failed|err):\s*([^\n]+)").unwrap();
    for cap in re.captures_iter(input) {
        if let Some(err) = cap.get(1) {
            entities.error_messages.push(err.as_str().trim().to_string());
        }
    }

    // 匹配错误码
    let re = Regex::new(r"E\d{4}").unwrap();
    for cap in re.find_iter(input) {
        entities.error_messages.push(cap.as_str().to_string());
    }
}

fn parse_code_snippets(input: &str, entities: &mut ExtractedEntities) {
    // 匹配 `code` 格式的代码片段
    let re = Regex::new(r#"`([^`]+)`"#).unwrap();
    for cap in re.captures_iter(input) {
        if let Some(snippet) = cap.get(1) {
            entities.code_snippets.push(snippet.as_str().to_string());
        }
    }
}

fn parse_functions(input: &str, entities: &mut ExtractedEntities) {
    // 匹配函数定义和调用
    let re = Regex::new(r"(?:fn|func|def|function)\s+([a-zA-Z_]\w*)").unwrap();
    for cap in re.captures_iter(input) {
        if let Some(name) = cap.get(1) {
            entities.functions.push(name.as_str().to_string());
        }
    }
}

fn extract_parameters(input: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();

    // 提取 key=value 参数
    let re = Regex::new(r"(\w+)=(\S+)").unwrap();
    for cap in re.captures_iter(input) {
        if let (Some(key), Some(value)) = (cap.get(1), cap.get(2)) {
            params.insert(key.as_str().to_string(), value.as_str().to_string());
        }
    }

    params
}

// ===== 任务创建 =====

async fn create_task_from_input(input: &str, state: &mut ReplState, executor: Arc<Executor>) {
    let title: String = if let Some(target) = &state.entities.task_names.last() {
        (*target).clone()
    } else {
        // 从输入中提取标题
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

fn generate_task_suggestions(input: &str, state: &mut ReplState) {
    state.task_suggestions.clear();

    let lower = input.to_lowercase();

    // 基于输入生成建议
    if lower.contains("test") {
        state.task_suggestions.push(TaskSuggestion {
            title: "Run and verify tests".to_string(),
            description: "Execute test suite for current changes".to_string(),
            priority: "Medium".to_string(),
            confidence: 0.85,
        });
    }

    if lower.contains("error") || lower.contains("bug") {
        state.task_suggestions.push(TaskSuggestion {
            title: "Debug and fix issue".to_string(),
            description: "Investigate and resolve the reported issue".to_string(),
            priority: "High".to_string(),
            confidence: 0.90,
        });
    }

    if lower.contains("file") || lower.contains("code") {
        state.task_suggestions.push(TaskSuggestion {
            title: "Review code changes".to_string(),
            description: "Review modified files and ensure quality".to_string(),
            priority: "Medium".to_string(),
            confidence: 0.75,
        });
    }

    if lower.contains("git") || lower.contains("commit") {
        state.task_suggestions.push(TaskSuggestion {
            title: "Create git commit".to_string(),
            description: "Stage and commit changes".to_string(),
            priority: "Low".to_string(),
            confidence: 0.80,
        });
    }
}

// ===== 辅助函数 =====

fn print_thought_process(parsed: &ParsedIntent, state: &ReplState) {
    println!("[Thinking...]");
    println!("  Action: {:?}", parsed.action_type);
    println!("  Confidence: {:.0}%", parsed.confidence * 100.0);

    if !state.entities.file_paths.is_empty() {
        println!("  Files: {}", state.entities.file_paths.join(", "));
    }

    if !state.entities.task_names.is_empty() {
        println!("  Task names: {}", state.entities.task_names.join(", "));
    }

    if !parsed.parameters.is_empty() {
        println!("  Params: {:?}", parsed.parameters);
    }
}

fn show_help() {
    println!(r#"
Available Commands:
  /help, /h          Show this help
  /status, /st        Show current session status
  /tasks              List all tasks
  /switch <role>      Switch agent role
  /verbose, /v        Toggle thought display
  /clear, /cls        Clear screen
  /new                Start new session
  /context, /ctx      Show context
  /suggest, /sg       Show task suggestions
  /exit, /quit, /q   Exit REPL

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
    println!("  Commands: {}", config.prompt.trim_end());
    println!("  Auto-create tasks: {}", config.auto_create_task);
    println!("  Dialogue entries: {}", state.dialogue_history.len());
    println!("  Created tasks: {}", state.created_tasks.len());
    println!("  Files referenced: {}", state.entities.file_paths.len());
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
    println!("  Tasks created: {}", state.created_tasks.len());
    println!("  Entities:");
    println!("    Files: {:?}", state.entities.file_paths);
    println!("    Functions: {:?}", state.entities.functions);
    println!("    Errors: {:?}", state.entities.error_messages);
}

fn show_suggestions(state: &ReplState) {
    if state.task_suggestions.is_empty() {
        println!("No suggestions. Try describing what you want to do.");
        return;
    }

    println!("Task Suggestions:");
    for (i, suggestion) in state.task_suggestions.iter().enumerate() {
        println!("  {}. {} [{}]", i + 1, suggestion.title, suggestion.priority);
        println!("     {}", suggestion.description);
        println!("     Confidence: {:.0}%", suggestion.confidence * 100.0);
    }
}

fn clear_screen() {
    print!("\x1B[2J\x1B[3J\x1B[H");
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_type_detection() {
        let mut parsed = ParsedIntent::default();
        detect_action_type("create a task to add user authentication", &mut parsed);
        assert_eq!(parsed.action_type, ActionType::CreateTask);
        assert!(parsed.confidence > 0.7);

        let mut parsed = ParsedIntent::default();
        detect_action_type("run test the code", &mut parsed);
        assert_eq!(parsed.action_type, ActionType::RunTests);

        let mut parsed = ParsedIntent::default();
        detect_action_type("fix the error in database.rs", &mut parsed);
        assert_eq!(parsed.action_type, ActionType::Debug);
    }

    #[test]
    fn test_file_path_parsing() {
        let mut entities = ExtractedEntities::default();
        parse_file_paths("edit src/main.rs and update Cargo.toml", &mut entities);
        assert!(entities.file_paths.contains(&"src/main.rs".to_string()));
        assert!(entities.file_paths.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn test_error_message_parsing() {
        let mut entities = ExtractedEntities::default();
        parse_error_messages("error: failed to compile with error E0425", &mut entities);
        assert!(!entities.error_messages.is_empty());
        assert!(entities.error_messages.iter().any(|e| e.contains("E0425")));
    }

    #[test]
    fn test_function_parsing() {
        let mut entities = ExtractedEntities::default();
        parse_functions("fn main() { }", &mut entities);
        assert!(entities.functions.contains(&"main".to_string()));
    }

    #[test]
    fn test_code_snippet_parsing() {
        let mut entities = ExtractedEntities::default();
        parse_code_snippets("use the `Executor::new` function", &mut entities);
        assert!(entities.code_snippets.contains(&"Executor::new".to_string()));
    }

    #[test]
    fn test_task_name_parsing() {
        let mut parsed = ParsedIntent::default();
        let mut entities = ExtractedEntities::default();
        parse_task_names("Create a task called \"Add User Authentication\"", &mut parsed, &mut entities);
        assert!(entities.task_names.contains(&"Add User Authentication".to_string()));
    }

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
    fn test_extracted_entities_default() {
        let entities = ExtractedEntities::default();
        assert!(entities.file_paths.is_empty());
        assert!(entities.functions.is_empty());
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
    fn test_task_suggestion() {
        let suggestion = TaskSuggestion {
            title: "Test Task".to_string(),
            description: "Test description".to_string(),
            priority: "High".to_string(),
            confidence: 0.9,
        };
        assert_eq!(suggestion.title, "Test Task");
        assert_eq!(suggestion.priority, "High");
    }

    #[test]
    fn test_calculate_confidence() {
        let conf = calculate_confidence("test", "test the code");
        assert!(conf > 0.7);

        let conf = calculate_confidence("create", "I want to create a new task");
        assert!(conf > 0.5);
    }

    #[test]
    fn test_extract_parameters() {
        let params = extract_parameters("test name=value foo=bar");
        assert_eq!(params.get("name"), Some(&"value".to_string()));
        assert_eq!(params.get("foo"), Some(&"bar".to_string()));
    }
}
