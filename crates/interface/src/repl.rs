//! REPL - OpenCode-style Natural Language Interaction
//!
//! 职责：
//! - OpenCode 风格的自然语言 REPL
//! - AI Agent 默认启用，直接处理所有用户输入
//! - 人类用户只需要用自然语言描述需求
//! - AI 自动理解意图并执行操作
//!
//! 设计理念 (来自 OpenCode):
//! - Agent 模式默认启用，无需手动开启
//! - 用户输入直接发送给 AI，不经过命令解析
//! - 移除 /create, /list, /run 等人类命令
//! - 简洁的 UI，专注于自然语言交互

use std::path::PathBuf;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// Agent mode integration
use crate::agent_mode::{
    AgentModeManager,
    AgentModeConfig,
    handle_agent_command,
};

/// REPL 配置 (OpenCode 风格 - 极简)
#[derive(Debug, Clone)]
pub struct ReplConfig {
    /// 提示符
    pub prompt: String,

    /// 是否显示思考过程
    pub show_thought: bool,

    /// 会话超时（秒）
    pub session_timeout: u64,

    /// 历史文件路径
    pub history_file: PathBuf,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            prompt: "> ".to_string(),
            show_thought: true,
            session_timeout: 3600,
            history_file: PathBuf::from(".ndc/repl_history"),
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

/// 运行 REPL (OpenCode 风格)
pub async fn run_repl(history_file: PathBuf, executor: Arc<ndc_runtime::Executor>) {
    let config = ReplConfig::new(history_file);

    // 创建 Agent Mode Manager (OpenCode 风格: 默认启用)
    let agent_manager = Arc::new(AgentModeManager::new(
        executor.clone(),
        Arc::new(ndc_runtime::tools::ToolRegistry::new()),
    ));

    // 启动时自动启用 Agent 模式
    let agent_config = AgentModeConfig::default();
    if let Err(e) = agent_manager.enable(agent_config).await {
        println!("[Warning] Failed to enable agent mode: {}", e);
    }

    // 打印欢迎信息 (OpenCode 风格: 极简)
    println!(r#"
NDC - Neo Development Companion

Natural language AI assistant. Just describe what you want.

Examples:
  "Create a REST API for user management"
  "Fix the bug in the login function"
  "Run tests for the authentication module"

Commands: /help, /model, /agent, /status, /clear, exit
"#);

    // REPL 循环
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("{}", config.prompt);
        io::stdout().flush().unwrap();

        input.clear();

        match stdin.lock().read_line(&mut input) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                // 处理退出
                if input == "exit" || input == "quit" || input == "q" {
                    println!("Goodbye!");
                    break;
                }

                // 处理命令
                if input.starts_with('/') {
                    handle_command(input, &config, agent_manager.clone()).await;
                } else {
                    // 自然语言输入 - 直接发送给 AI Agent
                    handle_agent_dialogue(input, &agent_manager).await;
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

// ===== 命令处理 (极简版) =====

async fn handle_command(input: &str, _config: &ReplConfig, agent_manager: Arc<AgentModeManager>) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "/help" | "/h" => show_help(),
        "/model" | "/m" => {
            if parts.len() > 1 {
                // 解析 provider/model 格式
                let model_spec = parts[1];
                let (provider, model) = if let Some(idx) = model_spec.find('/') {
                    (&model_spec[..idx], Some(&model_spec[idx+1..]))
                } else {
                    (model_spec, None)
                };
                if let Err(e) = agent_manager.switch_provider(provider, model).await {
                    println!("[Error] Failed to switch provider: {}", e);
                }
            } else {
                show_model_info();
            }
        }
        "/status" | "/st" => {
            let status = agent_manager.status().await;
            show_agent_status(&status);
        }
        "/agent" => {
            // OpenCode 风格: /agent 命令用于管理 agent 而非切换模式
            let agent_input = if parts.len() > 1 {
                format!("/agent {}", &input[7..])
            } else {
                "/agent help".to_string()
            };
            if let Err(e) = handle_agent_command(&agent_input, &agent_manager).await {
                println!("[Error] {}", e);
            }
        }
        "/clear" | "/cls" => {
            print!("\x1B[2J\x1B[3J\x1B[H");
            let _ = io::stdout().flush();
        }
        _ => {
            // 未知命令，尝试作为自然语言处理
            println!("[Tip] Unknown command. Use natural language or type '/help' for commands.");
        }
    }
}

// ===== Agent 对话处理 =====

/// 处理用户输入 (OpenCode 风格: 直接发送给 AI)
async fn handle_agent_dialogue(input: &str, agent_manager: &Arc<AgentModeManager>) {
    // 直接将用户输入发送给 AI Agent
    match agent_manager.process_input(input).await {
        Ok(response) => {
            // 打印 AI 响应
            if !response.content.is_empty() {
                println!();
                println!("{}", response.content);
            }

            // 显示工具调用
            if !response.tool_calls.is_empty() {
                let tool_names: Vec<&str> = response.tool_calls.iter()
                    .map(|t| t.name.as_str())
                    .collect();
                println!("\n[Tools: {}]", tool_names.join(", "));
            }

            // 显示验证结果
            if let Some(verification) = response.verification_result {
                match verification {
                    ndc_core::VerificationResult::Completed => {
                        println!("[OK] Verification passed!");
                    }
                    ndc_core::VerificationResult::Incomplete { reason } => {
                        println!("[Incomplete] {}", reason);
                    }
                    ndc_core::VerificationResult::QualityGateFailed { reason } => {
                        println!("[Failed] Quality gate: {}", reason);
                    }
                }
            }

            println!();
        }
        Err(e) => {
            // 显示详细的错误信息，包括配置来源
            show_agent_error(&e, agent_manager).await;
        }
    }
}

/// 显示详细的 Agent 错误信息
async fn show_agent_error(error: &ndc_core::AgentError, agent_manager: &Arc<AgentModeManager>) {
    let status = agent_manager.status().await;
    let error_msg = error.to_string();

    let display_error = if error_msg.len() > 50 { &error_msg[..50] } else { &error_msg };

    println!();
    println!("+--------------------------------------------------------------------+");
    println!("|  Agent Error                                                        |");
    println!("+--------------------------------------------------------------------+");
    println!("|  {}                 ", display_error);
    println!("+--------------------------------------------------------------------+");
    println!("|  Provider Configuration:                                            |");
    println!("|    Provider: {}                                                    ", status.provider);
    println!("|    Model: {}                                                       ", status.model);
    println!("+--------------------------------------------------------------------+");
    println!("|  Configuration Sources Checked:                                     |");

    // 检查各配置来源
    let provider_upper = status.provider.to_uppercase();

    // 环境变量
    let api_key_env = format!("NDC_{}_API_KEY", provider_upper);
    let group_id_env = format!("NDC_{}_GROUP_ID", provider_upper);

    let api_key_set = std::env::var(&api_key_env).is_ok();
    let group_id_set = std::env::var(&group_id_env).is_ok();

    println!("|    Env: {}={}                 ", api_key_env, if api_key_set { "[SET]" } else { "[NOT SET]" });
    println!("|    Env: {}={}      ", group_id_env, if group_id_set { "[SET]" } else { "[NOT SET]" });

    // 检查配置文件
    let config_paths = [
        ("Project", ".ndc/config.yaml"),
        ("User", "~/.config/ndc/config.yaml"),
        ("Global", "/etc/ndc/config.yaml"),
    ];

    for (name, path) in &config_paths {
        let expanded = if path.starts_with("~") {
            if let Ok(home) = std::env::var("HOME") {
                path.replace("~", &home)
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        };

        let exists = std::path::Path::new(&expanded).exists();
        println!("|    {} Config: [{}] {}                 ", name, if exists { "FOUND" } else { "NOT FOUND" }, path);
    }

    println!("+--------------------------------------------------------------------+");
    println!("|  How to Fix:                                                        |");
    println!("|    1. Set API key: export NDC_{}_API_KEY=\"your-key\"           ", provider_upper);
    if provider_upper == "MINIMAX" {
        println!("|    2. Set Group ID: export NDC_{}_GROUP_ID=\"your-group-id\"  ", provider_upper);
    }
    println!("|    3. Restart REPL or try: /model openai                          ");
    println!("+--------------------------------------------------------------------+");
    println!();
}

// ===== 辅助函数 =====

fn show_help() {
    println!(r#"
Available Commands:
  /help, /h       Show this help
  /model, /m      Show or switch LLM (e.g., /model minimax/m2.1-0107)
  /agent          Manage agent settings
  /status         Show agent status
  /clear          Clear screen
  exit, quit, q   Exit REPL

Natural Language Examples:
  "Create a REST API for user management"
  "Fix the bug in authentication"
  "Run tests for the payment module"
  "Explain how the system works"

LLM Providers: minimax, openrouter, openai, anthropic, ollama

Environment Variables:
  NDC_MINIMAX_API_KEY, NDC_OPENAI_API_KEY, etc.
"#);
}

fn show_model_info() {
    println!("Current Model Configuration:");
    println!();
    println!("Available providers: openai, anthropic, minimax, openrouter, ollama");
    println!();
    println!("Usage: /model <provider>[/<model>]");
    println!();
    println!("Examples:");
    println!("  /model minimax");
    println!("  /model minimax/m2.1-0107");
    println!("  /model openrouter");
    println!("  /model openai/gpt-4o");
    println!();
    println!("Environment Variables:");
    println!("  NDC_OPENAI_API_KEY, NDC_OPENAI_MODEL");
    println!("  NDC_ANTHROPIC_API_KEY, NDC_ANTHROPIC_MODEL");
    println!("  NDC_MINIMAX_API_KEY, NDC_MINIMAX_GROUP_ID, NDC_MINIMAX_MODEL");
    println!("  NDC_OPENROUTER_API_KEY, NDC_OPENROUTER_MODEL");
    println!("  NDC_OLLAMA_MODEL, NDC_OLLAMA_URL");
}

fn show_agent_status(status: &crate::agent_mode::AgentModeStatus) {
    println!();
    println!("+--------------------------------------------------------------------+");
    println!("|  Agent Status                                                        |");
    println!("+--------------------------------------------------------------------+");
    println!("|  Status: {}                                                         ",
        if status.enabled { "Enabled" } else { "Disabled" });
    if status.enabled {
        println!("|  Agent: {}                                                         ", status.agent_name);
        println!("|  Provider: {} @ {}                                                  ", status.provider, status.model);
        if let Some(sid) = &status.session_id {
            println!("|  Session: {}                                                      ", sid);
        }
    }
    println!("+--------------------------------------------------------------------+");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_state_default() {
        let state = ReplState::default();
        assert!(!state.session_id.is_empty());
        assert!(state.current_provider.is_none());
        assert!(state.current_model.is_none());
    }

    #[test]
    fn test_repl_state_not_expired() {
        let state = ReplState::default();
        assert!(!state.is_expired(3600));
        assert!(state.is_expired(0));
    }

    #[test]
    fn test_repl_config_default() {
        let config = ReplConfig::default();
        assert_eq!(config.prompt, "> ");
        assert!(config.show_thought);
        assert_eq!(config.session_timeout, 3600);
    }
}
