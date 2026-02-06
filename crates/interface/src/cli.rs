//! CLI - 命令行接口
//!
//! 职责：
//! - 任务管理命令（create, list, status, logs）
//! - REPL 启动
//! - 守护进程控制

use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn, error};

/// CLI 错误
#[derive(Debug, Error)]
pub enum CliError {
    #[error("执行器初始化失败: {0}")]
    ExecutorInitFailed(String),

    #[error("任务执行失败: {0}")]
    TaskExecutionFailed(String),

    #[error("存储错误: {0}")]
    StorageError(String),
}

/// CLI 配置
#[derive(Debug, Clone)]
pub struct CliConfig {
    /// 项目根目录
    pub project_root: PathBuf,

    /// 存储路径
    pub storage_path: PathBuf,

    /// 是否启用详细输出
    pub verbose: bool,

    /// 输出格式
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Pretty,
    Json,
    Minimal,
}

/// NDC CLI
#[derive(Parser, Debug)]
#[command(name = "ndc")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// 项目根目录
    #[arg(short, long, global = true)]
    project_root: Option<PathBuf>,

    /// 存储路径
    #[arg(short, long, global = true)]
    storage: Option<PathBuf>,

    /// 详细输出
    #[arg(short, long, global = true)]
    verbose: bool,

    /// 输出格式
    #[arg(long, global = true, value_enum)]
    output: Option<OutputFormat>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 创建新任务
    Create(CreateArgs),

    /// 列出任务
    List(ListArgs),

    /// 查看任务状态
    Status(StatusArgs),

    /// 查看任务日志
    Logs(LogArgs),

    /// 执行任务
    Run(RunArgs),

    /// 回滚任务
    Rollback(RollbackArgs),

    /// 启动 REPL
    Repl(ReplArgs),

    /// 启动守护进程
    Daemon(DaemonArgs),

    /// 搜索记忆
    Search(SearchArgs),

    /// 查看系统状态
    StatusSystem,
}

#[derive(Args, Debug)]
struct CreateArgs {
    /// 任务标题
    title: String,

    /// 任务描述
    #[arg(short, long)]
    description: Option<String>,

    /// 任务类型
    #[arg(short, long)]
    task_type: Option<String>,
}

#[derive(Args, Debug)]
struct ListArgs {
    /// 状态过滤
    #[arg(short, long)]
    state: Option<String>,

    /// 限制数量
    #[arg(short, long, default_value = "20")]
    limit: u32,
}

#[derive(Args, Debug)]
struct StatusArgs {
    /// 任务 ID
    task_id: Option<String>,
}

#[derive(Args, Debug)]
struct LogArgs {
    /// 任务 ID
    task_id: String,

    /// 行数限制
    #[arg(short, long, default_value = "50")]
    lines: u32,
}

#[derive(Args, Debug)]
struct RunArgs {
    /// 任务 ID
    task_id: String,

    /// 同步执行（等待完成）
    #[arg(short, long)]
    sync: bool,
}

#[derive(Args, Debug)]
struct RollbackArgs {
    /// 任务 ID
    task_id: String,

    /// 快照 ID（默认最新）
    snapshot_id: Option<String>,
}

#[derive(Args, Debug)]
struct ReplArgs {
    /// 历史文件路径
    #[arg(short, long)]
    history: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct DaemonArgs {
    /// 监听地址
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    address: String,

    /// 后台运行
    #[arg(short, long)]
    background: bool,
}

#[derive(Args, Debug)]
struct SearchArgs {
    /// 搜索查询
    query: String,

    /// 稳定性过滤
    #[arg(short, long)]
    stability: Option<String>,

    /// 限制数量
    #[arg(short, long, default_value = "10")]
    limit: u32,
}

/// 运行 CLI
pub async fn run_cli() -> Result<(), CliError> {
    let args = Cli::parse();

    let config = CliConfig {
        project_root: args.project_root.unwrap_or(PathBuf::from(".")),
        storage_path: args.storage.unwrap_or(PathBuf::from(".ndc/storage")),
        verbose: args.verbose,
        output_format: args.output.unwrap_or(OutputFormat::Pretty),
    };

    // 初始化跟踪
    if config.verbose {
        tracing_subscriber::fmt::init();
    }

    // 执行命令
    match args.command {
        Commands::Create(args) => cmd_create(args, &config).await,
        Commands::List(args) => cmd_list(args, &config).await,
        Commands::Status(args) => cmd_status(args, &config).await,
        Commands::Logs(args) => cmd_logs(args, &config).await,
        Commands::Run(args) => cmd_run(args, &config).await,
        Commands::Rollback(args) => cmd_rollback(args, &config).await,
        Commands::Repl(args) => cmd_repl(args, &config).await,
        Commands::Daemon(args) => cmd_daemon(args, &config).await,
        Commands::Search(args) => cmd_search(args, &config).await,
        Commands::StatusSystem => cmd_status_system(&config).await,
    }
}

// ===== 命令实现 =====

async fn cmd_create(args: CreateArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Creating task: {}", args.title);

    // TODO: 实现任务创建
    // 1. 初始化存储
    // 2. 创建任务
    // 3. 保存到存储

    println!("✅ Task created successfully!");
    Ok(())
}

async fn cmd_list(args: ListArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Listing tasks (limit: {})", args.limit);

    // TODO: 实现任务列表
    println!("📋 Tasks:");
    println!("  No tasks found.");

    Ok(())
}

async fn cmd_status(args: StatusArgs, config: &CliConfig) -> Result<(), CliError> {
    let task_id = args.task_id.unwrap_or_else(|| "latest".to_string());
    info!("Getting status for task: {}", task_id);

    // TODO: 实现任务状态
    println!("ℹ️  Task {}: Unknown", task_id);

    Ok(())
}

async fn cmd_logs(args: LogArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Getting logs for task: {} ({} lines)", args.task_id, args.lines);

    // TODO: 实现日志查看
    println!("📜 Logs for {}:", args.task_id);
    println!("  [No logs available]");

    Ok(())
}

async fn cmd_run(args: RunArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Running task: {}", args.task_id);

    if args.sync {
        println!("🔄 Executing task synchronously...");
    } else {
        println!("🚀 Task submitted for execution");
    }

    // TODO: 实现任务执行
    Ok(())
}

async fn cmd_rollback(args: RollbackArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Rolling back task: {}", args.task_id);

    // TODO: 实现回滚
    println!("🔙 Rollback initiated for task {}", args.task_id);

    Ok(())
}

async fn cmd_repl(args: ReplArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Starting REPL...");

    // 启动 REPL
    let history = args.history.unwrap_or_else(|| PathBuf::from(".ndc/repl_history"));
    super::run_repl(history).await;

    Ok(())
}

async fn cmd_daemon(args: DaemonArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Starting daemon on: {}", args.address);

    // 启动守护进程
    let address = args.address.parse().unwrap();
    super::run_daemon(address).await;

    Ok(())
}

async fn cmd_search(args: SearchArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Searching memory: {}", args.query);

    // TODO: 实现记忆搜索
    println!("🔍 Search results for '{}':", args.query);
    println!("  No matches found.");

    Ok(())
}

async fn cmd_status_system(config: &CliConfig) -> Result<(), CliError> {
    println!("📊 NDC System Status:");
    println!("  Storage: {:?}", config.storage_path);
    println!("  Project: {:?}", config.project_root);

    Ok(())
}
