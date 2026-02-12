//! CLI - Command Line Interface
//!
//! Design Philosophy (from OpenCode):
//! - Human users interact via natural language REPL
//! - AI automatically manages tasks internally
//! - Task system is an internal workflow mechanism, not human commands
//!
//! Available Commands:
//! - ndc run "message"  - Run AI with a message (one-shot or REPL)
//! - ndc repl           - Start interactive REPL
//! - ndc daemon         - Start background daemon
//!
//! Removed Commands (now AI internal workflow):
//! - create, list, status, logs, run, rollback (use natural language instead)

use clap::{Parser, Subcommand, Args, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, error};

use ndc_core::AgentRole;
use ndc_runtime::{Executor, ExecutionContext, MemoryStorage};

/// CLI Errors
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CliError {
    #[error("Executor initialization failed: {0}")]
    ExecutorInitFailed(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// CLI Configuration
#[derive(Debug, Clone)]
pub struct CliConfig {
    /// Project root directory
    pub project_root: PathBuf,

    /// Storage path
    pub storage_path: PathBuf,

    /// Verbose output
    pub verbose: bool,

    /// Output format
    pub output_format: OutputFormat,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            storage_path: PathBuf::from(".ndc/storage"),
            verbose: false,
            output_format: OutputFormat::Pretty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Pretty,
    Json,
    Minimal,
}

/// NDC CLI
#[derive(Parser, Debug)]
#[command(name = "ndc")]
#[command(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Project root directory
    #[arg(short, long, global = true)]
    project_root: Option<PathBuf>,

    /// Storage path
    #[arg(short, long, global = true)]
    storage: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format
    #[arg(long, global = true, value_enum)]
    output: Option<OutputFormat>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Run AI with a message (one-shot or interactive)
    Run(RunArgs),

    /// Start interactive REPL
    Repl(ReplArgs),

    /// Start background daemon
    Daemon(DaemonArgs),

    /// Search memory (AI internal)
    Search(SearchArgs),

    /// Show system status
    StatusSystem,
}

#[derive(Args, Debug)]
pub(crate) struct RunArgs {
    /// Message to send to AI
    #[arg(short, long)]
    pub message: Option<String>,

    /// Continue last session
    #[arg(short, long)]
    pub continue_session: bool,

    /// Session ID to continue
    #[arg(short, long)]
    pub session: Option<String>,

    /// Model to use (provider/model format)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Agent to use
    #[arg(short, long)]
    pub agent: Option<String>,

    /// Non-interactive mode (no REPL)
    #[arg(long)]
    pub one_shot: bool,
}

#[derive(Args, Debug)]
pub(crate) struct ReplArgs {
    /// History file path
    #[arg(long)]
    pub history: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub(crate) struct DaemonArgs {
    /// Listen address
    #[arg(short, long, default_value = "127.0.0.1:4096")]
    pub address: String,
}

#[derive(Args, Debug)]
pub(crate) struct SearchArgs {
    /// Search query
    pub query: String,
}

/// Parse CLI arguments and execute commands
pub async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();

    // Build config from args
    let config = CliConfig {
        project_root: cli.project_root.unwrap_or_else(|| PathBuf::from(".")),
        storage_path: cli.storage.unwrap_or_else(|| PathBuf::from(".ndc/storage")),
        verbose: cli.verbose,
        output_format: cli.output.unwrap_or(OutputFormat::Pretty),
    };

    if config.verbose {
        tracing_subscriber::fmt::init();
    }

    match cli.command {
        Commands::Run(args) => cmd_run(args, &config).await,
        Commands::Repl(args) => cmd_repl(args, &config).await,
        Commands::Daemon(args) => cmd_daemon(args, &config).await,
        Commands::Search(args) => cmd_search(args).await,
        Commands::StatusSystem => cmd_status_system().await,
    }
}

async fn cmd_run(args: RunArgs, config: &CliConfig) -> Result<(), CliError> {
    // Initialize executor for tool access
    let context = ExecutionContext {
        storage: Arc::new(MemoryStorage::new()),
        workflow_engine: Arc::new(ndc_runtime::WorkflowEngine::new()),
        tools: Arc::new(ndc_runtime::ToolManager::new()),
        quality_runner: Arc::new(ndc_runtime::QualityGateRunner::new()),
        project_root: config.project_root.clone(),
        current_role: AgentRole::Historian,
    };
    let executor = Arc::new(Executor::new(context));

    if let Some(msg) = args.message {
        // One-shot mode: send message and exit
        info!("Running one-shot: {}", msg);

        // TODO: Call AI with message and get response
        println!("[AI] Processing: {}", msg);
        println!("[Note] Full AI integration coming soon.");

        Ok(())
    } else {
        // Interactive REPL mode
        info!("Starting REPL...");
        let history = PathBuf::from(".ndc/repl_history");
        super::run_repl(history, executor).await;
        Ok(())
    }
}

async fn cmd_repl(args: ReplArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Starting REPL...");

    // Initialize executor for tool access
    let context = ExecutionContext {
        storage: Arc::new(MemoryStorage::new()),
        workflow_engine: Arc::new(ndc_runtime::WorkflowEngine::new()),
        tools: Arc::new(ndc_runtime::ToolManager::new()),
        quality_runner: Arc::new(ndc_runtime::QualityGateRunner::new()),
        project_root: config.project_root.clone(),
        current_role: AgentRole::Historian,
    };
    let executor = Arc::new(Executor::new(context));

    // Start REPL
    let history = args.history.unwrap_or_else(|| PathBuf::from(".ndc/repl_history"));
    super::run_repl(history, executor).await;

    Ok(())
}

async fn cmd_daemon(args: DaemonArgs, _config: &CliConfig) -> Result<(), CliError> {
    info!("Starting daemon on: {}", args.address);

    // Start daemon
    let address = args.address.parse().unwrap();
    super::run_daemon(address).await;

    Ok(())
}

async fn cmd_search(args: SearchArgs) -> Result<(), CliError> {
    info!("Searching memory: {}", args.query);

    println!("Search results for '{}':", args.query);
    println!("  (AI memory search - full integration coming soon)");

    Ok(())
}

async fn cmd_status_system() -> Result<(), CliError> {
    println!("NDC System Status:");
    println!("  Mode: AI Agent (natural language interaction)");
    println!("  REPL: Use 'ndc repl' for interactive mode");
    println!("  One-shot: Use 'ndc run --message \"...\"' for single messages");
    println!();
    println!("Design Philosophy (from OpenCode):");
    println!("  - Human users interact via natural language");
    println!("  - AI automatically manages tasks internally");
    println!("  - Task commands removed from CLI (use natural language instead)");

    Ok(())
}
