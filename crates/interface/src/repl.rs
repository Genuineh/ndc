//! REPL - 交互式对话模式
//!
//! 职责：
//! - 持续对话
//! - 意图解析
//! - 任务创建与执行
//! - 上下文显示

use std::path::PathBuf;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use ndc_core::{AgentRole, Task, Intent, Action};
use ndc_decision::DecisionEngine;
use ndc_runtime::Executor;
use tracing::{info, warn, debug};

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
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            history_file: PathBuf::from(".ndc/repl_history"),
            max_history: 1000,
            show_thought: true,
            prompt: "ndc> ".to_string(),
        }
    }
}

/// REPL 状态
#[derive(Debug, Clone)]
pub struct ReplState {
    /// 当前任务
    pub current_task: Option<Task>,

    /// 对话历史
    pub dialogue_history: Vec<DialogueEntry>,

    /// 角色
    pub role: AgentRole,
}

#[derive(Debug, Clone)]
pub struct DialogueEntry {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 运行 REPL
pub async fn run_repl(history_file: PathBuf) {
    let config = ReplConfig {
        history_file,
        ..Default::default()
    };

    // 初始化组件
    let executor = Arc::new(ndc_runtime::Executor::default());
    let decision_engine = Arc::new(ndc_decision::BasicDecisionEngine::new());

    // 初始化存储（可选，失败不影响 REPL 运行）
    let _storage_path = PathBuf::from(".ndc/storage");
    warn!("Storage initialization skipped - running in memory-only mode");

    info!("Starting NDC REPL (type 'help' for commands, 'exit' to quit)");

    // 打印欢迎信息
    println!(r#"
╔═══════════════════════════════════════════════════════════════════╗
║  NDC - Neo Development Companion                               ║
║  Type 'help' for commands, 'exit' to quit                     ║
╚═══════════════════════════════════════════════════════════════════╝
"#);

    // REPL 循环
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("{}", config.prompt);
        io::stdout().flush().unwrap();

        input.clear();
        match stdin.lock().read_line(&mut input) {
            Ok(0) => break,  // EOF
            Ok(_) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                // 加载历史
                load_history(&config.history_file);

                // 处理命令或对话
                if input.starts_with('/') {
                    if let Err(e) = handle_command(input, &mut ReplState::default()).await {
                        error!("Command failed: {}", e);
                    }
                } else {
                    // 作为自然语言处理
                    if let Err(e) = handle_dialogue(input, &mut ReplState::default()).await {
                        error!("Dialogue failed: {}", e);
                    }
                }

                // 保存历史
                save_history(&config.history_file);
            }
            Err(e) => {
                error!("Read error: {}", e);
                break;
            }
        }
    }

    info!("REPL exited");
}

// ===== 命令处理 =====

async fn handle_command(input: &str, state: &mut ReplState) -> Result<(), String> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "/help" => show_help(),
        "/exit" | "/quit" => {
            println!("Goodbye!");
            std::process::exit(0);
        }
        "/status" => show_status(state),
        "/tasks" => list_tasks(state).await,
        "/switch" if parts.len() > 1 => switch_role(parts[1], state),
        "/verbose" => toggle_verbose(state),
        "/clear" => clear_screen(),
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

async fn handle_dialogue(input: &str, state: &mut ReplState) -> Result<(), String> {
    // 记录对话
    state.dialogue_history.push(DialogueEntry {
        role: "user".to_string(),
        content: input.to_string(),
        timestamp: chrono::Utc::now(),
    });

    // 解析意图
    let intent = parse_intent(input, state).await?;

    // 显示思考过程
    if state.dialogue_history.is_empty() {
        println!("[Thinking...]");
    }

    // 提交意图到决策引擎
    println!("[{}] Processing...", state.role);

    Ok(())
}

// ===== 辅助函数 =====

fn show_help() -> Result<(), String> {
    println!(r#"
Available commands:
  /help         Show this help
  /status       Show current task status
  /tasks        List all tasks
  /switch <role> Switch agent role (planner/implementer/reviewer/tester/historian)
  /verbose      Toggle thought display
  /clear        Clear screen
  /exit         Exit REPL

Natural language examples:
  "Create a new task to add user authentication"
  "Run tests for the API"
  "Search for memory about error handling"
"#);
    Ok(())
}

fn show_status(state: &ReplState) -> Result<(), String> {
    println!("Current state:");
    println!("  Role: {:?}", state.role);

    if let Some(task) = &state.current_task {
        println!("  Current task: {} ({:?})", task.id, task.state);
    } else {
        println!("  Current task: None");
    }

    println!("  Dialogue entries: {}", state.dialogue_history.len());

    Ok(())
}

async fn list_tasks(state: &ReplState) -> Result<(), String> {
    println!("📋 Tasks:");
    println!("  (Not implemented yet)");

    Ok(())
}

fn switch_role(role_str: &str, state: &mut ReplState) -> Result<(), String> {
    match role_str.to_lowercase().as_str() {
        "planner" => state.role = AgentRole::Planner,
        "implementer" => state.role = AgentRole::Implementer,
        "reviewer" => state.role = AgentRole::Reviewer,
        "tester" => state.role = AgentRole::Tester,
        "historian" => state.role = AgentRole::Historian,
        _ => return Err(format!("Unknown role: {}", role_str)),
    }

    println!("[Switched to {}]", state.role);
    Ok(())
}

fn toggle_verbose(state: &ReplState) -> Result<(), String> {
    Ok(())
}

fn clear_screen() -> Result<(), String> {
    print!("\x1B[2J\x1B[3J\x1B[H");
    io::stdout().flush().unwrap();
    Ok(())
}

async fn parse_intent(input: &str, state: &ReplState) -> Result<Intent, String> {
    // 简单意图解析
    let action = if input.contains("create") && input.contains("task") {
        Action::CreateTask {
            task_spec: ndc_core::TaskSpec {
                title: input.to_string(),
                description: input.to_string(),
                task_type: "general".to_string(),
            }
        }
    } else if input.contains("test") {
        Action::RunTests {
            test_type: ndc_core::TestType::All,
        }
    } else if input.contains("build") {
        Action::RunQualityCheck {
            check_type: ndc_core::QualityCheckType::Build,
        }
    } else if input.contains("search") || input.contains("find") {
        Action::SearchKnowledge {
            query: input.to_string(),
        }
    } else {
        Action::Other {
            name: "dialogue".to_string(),
            params: serde_json::json!({ "input": input }),
        }
    };

    Ok(Intent {
        id: ndc_core::IntentId::new(),
        agent: ndc_core::AgentId::new(),
        agent_role: state.role,
        proposed_action: action,
        effects: vec![],
        reasoning: input.to_string(),
        task_id: state.current_task.as_ref().map(|t| t.id),
        timestamp: chrono::Utc::now(),
    })
}

fn load_history(path: &PathBuf) {
    if !path.exists() {
        return;
    }

    // TODO: 实现历史加载
}

fn save_history(path: &PathBuf) {
    // TODO: 实现历史保存
}
