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

use std::io::IsTerminal;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[cfg(test)]
use crate::redaction::{RedactionMode, sanitize_text};

// Agent mode integration
use crate::agent_mode::{AgentModeConfig, AgentModeManager};

// TUI crate (extracted from crate::tui)
use ndc_tui::*;

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
            show_thought: false,
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
    let mut viz_state = ReplVisualizationState::new(config.show_thought);

    // 创建 Agent Mode Manager (OpenCode 风格: 默认启用)
    let agent_manager = Arc::new(AgentModeManager::new(
        executor.clone(),
        Arc::new(ndc_runtime::create_default_tool_registry_with_storage(
            executor.context().storage.clone(),
        )),
    ));

    // TUI 模式下设置权限确认通道（在 enable 之前）
    let is_tui = io::stdout().is_terminal() && std::env::var("NDC_REPL_LEGACY").is_err();

    // 加载永久批准的权限到安全网关环境变量
    let approved = ndc_core::NdcConfigLoader::load_approved_permissions();
    if !approved.is_empty() {
        let existing = std::env::var("NDC_SECURITY_OVERRIDE_PERMISSIONS").unwrap_or_default();
        let mut all: Vec<String> = if existing.is_empty() {
            Vec::new()
        } else {
            existing.split(',').map(|s| s.trim().to_string()).collect()
        };
        for p in &approved {
            if !all.contains(p) {
                all.push(p.clone());
            }
        }
        unsafe { std::env::set_var("NDC_SECURITY_OVERRIDE_PERMISSIONS", all.join(",")) };
    }

    let permission_rx = if is_tui {
        let (tx, rx) = tokio::sync::mpsc::channel::<TuiPermissionRequest>(4);
        <AgentModeManager as AgentBackend>::set_permission_channel(&*agent_manager, tx).await;
        Some(rx)
    } else {
        None
    };

    // 启动时自动启用 Agent 模式
    let agent_config = AgentModeConfig::default();
    if let Err(e) = agent_manager.enable(agent_config).await {
        println!("[Warning] Failed to enable agent mode: {}", e);
    }

    let agent_backend: Arc<dyn AgentBackend> = agent_manager.clone();

    if is_tui {
        if let Err(e) = run_repl_tui(
            &mut viz_state,
            agent_backend.clone(),
            permission_rx.unwrap(),
        )
        .await
        {
            warn!("TUI mode failed, fallback to legacy REPL: {}", e);
        } else {
            return;
        }
    }

    // 打印欢迎信息 (OpenCode 风格: 极简)
    println!(
        r#"
NDC - Neo Development Companion

Natural language AI assistant. Just describe what you want.

Examples:
  "Create a REST API for user management"
  "Fix the bug in the login function"
  "Run tests for the authentication module"

Commands: /help, /provider, /model, /agent, /status, /stream, /workflow, /tokens, /metrics, /clear, exit
"#
    );

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
                    handle_command(input, &mut viz_state, agent_backend.clone()).await;
                } else {
                    // 自然语言输入 - 直接发送给 AI Agent
                    handle_agent_dialogue(input, &agent_backend, &mut viz_state).await;
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
        assert!(!config.show_thought);
        assert_eq!(config.session_timeout, 3600);
    }

    #[test]
    fn test_sanitize_sensitive_text() {
        let input = "token=abc123 Bearer zyx987 sk-ABCDEF123456 /home/jerryg/project password:foo";
        let out = sanitize_text(input, RedactionMode::Basic);
        assert!(out.contains("token=[REDACTED]"));
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(out.contains("sk-[REDACTED]"));
        assert!(out.contains("/home/***"));
        assert!(out.contains("password=[REDACTED]"));
        assert!(!out.contains("abc123"));
        assert!(!out.contains("zyx987"));
    }
}
