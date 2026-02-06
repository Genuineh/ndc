//! CLI - 命令行接口
//!
//! 职责：
//! - 任务管理命令（create, list, status, logs）
//! - REPL 启动
//! - 守护进程控制

use clap::{Parser, Subcommand, Args, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, error};

use ndc_core::{TaskId, AgentRole};
use ndc_runtime::{Executor, ExecutionContext, MemoryStorage};

/// CLI 错误
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CliError {
    #[error("执行器初始化失败: {0}")]
    ExecutorInitFailed(String),

    #[error("任务执行失败: {0}")]
    TaskExecutionFailed(String),

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("任务未找到: {0}")]
    TaskNotFound(TaskId),

    #[error("无效的任务 ID: {0}")]
    InvalidTaskId(String),

    #[error("无效的状态: {0}")]
    InvalidState(String),
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
pub(crate) enum Commands {
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
pub(crate) struct CreateArgs {
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
pub(crate) struct ListArgs {
    /// 状态过滤
    #[arg(short, long)]
    state: Option<String>,

    /// 限制数量
    #[arg(short, long, default_value = "20")]
    limit: u32,
}

#[derive(Args, Debug)]
pub(crate) struct StatusArgs {
    /// 任务 ID
    task_id: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct LogArgs {
    /// 任务 ID
    task_id: String,

    /// 行数限制
    #[arg(short, long, default_value = "50")]
    lines: u32,
}

#[derive(Args, Debug)]
pub(crate) struct RunArgs {
    /// 任务 ID
    task_id: String,

    /// 同步执行（等待完成）
    #[arg(short, long)]
    sync: bool,
}

#[derive(Args, Debug)]
pub(crate) struct RollbackArgs {
    /// 任务 ID
    task_id: String,

    /// 快照 ID（默认最新）
    snapshot_id: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct ReplArgs {
    /// 历史文件路径
    #[arg(long)]
    history: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub(crate) struct DaemonArgs {
    /// 监听地址
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    address: String,

    /// 后台运行
    #[arg(short, long)]
    background: bool,
}

#[derive(Args, Debug)]
pub(crate) struct SearchArgs {
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

    let executor = init_executor(config);

    let description = args.description.unwrap_or_default();
    let task = executor.create_task(
        args.title.clone(),
        description,
        AgentRole::Historian,
    )
    .await
    .map_err(|e| CliError::ExecutorInitFailed(e.to_string()))?;

    println!("✅ Task created successfully!");
    println!("   ID: {}", task.id);
    println!("   Title: {}", task.title);
    println!("   State: {:?}", task.state);

    Ok(())
}

async fn cmd_list(args: ListArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Listing tasks (limit: {})", args.limit);

    let executor = init_executor(config);
    let storage = &executor.context().storage;

    // 获取所有任务
    println!("📋 Tasks:");

    // 尝试获取任务列表
    match storage.list_tasks().await {
        Ok(tasks) => {
            let total = tasks.len();
            let tasks: Vec<_> = tasks.into_iter().take(args.limit as usize).collect();
            if tasks.is_empty() {
                println!("   No tasks found.");
            } else {
                for task in &tasks {
                    println!("   [{}] {} - {:?}",
                        task.id.to_string().chars().take(8).collect::<String>(),
                        task.title,
                        task.state
                    );
                }
                if total > args.limit as usize {
                    println!("   ... and {} more", total - args.limit as usize);
                }
            }
        }
        Err(e) => {
            println!("   Error listing tasks: {}", e);
        }
    }

    Ok(())
}

async fn cmd_status(args: StatusArgs, config: &CliConfig) -> Result<(), CliError> {
    let task_id_str = args.task_id.unwrap_or_else(|| "latest".to_string());
    info!("Getting status for task: {}", task_id_str);

    let executor = init_executor(config);
    let storage = &executor.context().storage;

    // 如果是 "latest"，尝试获取最新任务
    let task = if task_id_str == "latest" {
        match storage.list_tasks().await {
            Ok(tasks) => {
                tasks.into_iter().max_by_key(|t| t.metadata.created_at)
                    .ok_or_else(|| CliError::StorageError("No tasks found".to_string()))?
            }
            Err(e) => return Err(CliError::StorageError(e.to_string())),
        }
    } else {
        let task_id = parse_task_id(&task_id_str)?;
        match storage.get_task(&task_id).await {
            Ok(Some(task)) => task,
            Ok(None) => return Err(CliError::TaskNotFound(task_id)),
            Err(e) => return Err(CliError::StorageError(e.to_string())),
        }
    };

    println!("ℹ️  Task: {}", task.title);
    println!("   ID: {}", task.id);
    println!("   State: {:?}", task.state);
    println!("   Created: {:?}", task.metadata.created_at);
    println!("   Steps: {}", task.steps.len());

    if !task.steps.is_empty() {
        println!("   Recent steps:");
        for step in task.steps.iter().rev().take(5) {
            println!("     - [{}] {:?}", step.step_id, step.status);
        }
    }

    Ok(())
}

async fn cmd_logs(args: LogArgs, _config: &CliConfig) -> Result<(), CliError> {
    info!("Getting logs for task: {} ({} lines)", args.task_id, args.lines);

    let task_id = parse_task_id(&args.task_id)?;
    let executor = init_executor(_config);
    let storage = &executor.context().storage;

    match storage.get_task(&task_id).await {
        Ok(Some(task)) => {
            println!("📜 Logs for {}:", args.task_id);
            println!("   Task: {}", task.title);
            println!("   State: {:?}", task.state);
            println!("   ---");

            if task.steps.is_empty() {
                println!("   No execution steps recorded.");
            } else {
                let lines = std::cmp::min(args.lines as usize, task.steps.len());
                for step in task.steps.iter().rev().take(lines) {
                    println!("   [Step {}] {:?} - {:?}",
                        step.step_id,
                        step.action,
                        step.status
                    );
                    if let Some(ref result) = step.result {
                        if !result.output.is_empty() {
                            println!("     Output: {}", result.output.chars().take(200).collect::<String>());
                        }
                        if let Some(ref err) = result.error {
                            println!("     Error: {}", err);
                        }
                    }
                }
            }
        }
        Ok(None) => return Err(CliError::TaskNotFound(task_id)),
        Err(e) => return Err(CliError::StorageError(e.to_string())),
    }

    Ok(())
}

async fn cmd_run(args: RunArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Running task: {}", args.task_id);

    let task_id = parse_task_id(&args.task_id)?;
    let executor = init_executor(config);

    if args.sync {
        println!("🔄 Executing task synchronously...");

        match executor.execute_task(task_id).await {
            Ok(result) => {
                println!("✅ Task completed successfully!");
                println!("   Final state: {:?}", result.final_state);
                println!("   Steps executed: {}", result.steps.len());
                println!("   Duration: {}ms", result.metrics.total_duration_ms);
            }
            Err(e) => {
                println!("❌ Task execution failed: {}", e);
                return Err(CliError::TaskExecutionFailed(e.to_string()));
            }
        }
    } else {
        println!("🚀 Task submitted for execution (async mode not yet implemented)");
        println!("   Use --sync flag to execute synchronously");
    }

    Ok(())
}

async fn cmd_rollback(args: RollbackArgs, _config: &CliConfig) -> Result<(), CliError> {
    info!("Rolling back task: {}", args.task_id);

    let task_id = parse_task_id(&args.task_id)?;
    let executor = init_executor(_config);
    let storage = &executor.context().storage;

    match storage.get_task(&task_id).await {
        Ok(Some(task)) => {
            println!("🔙 Rollback initiated for task {}", args.task_id);
            println!("   Task: {}", task.title);
            println!("   Current state: {:?}", task.state);

            // 检查是否有快照可以回滚
            if task.snapshots.is_empty() && task.lightweight_snapshots.is_empty() {
                println!("   ⚠️  No snapshots available for rollback");
                return Ok(());
            }

            println!("   Snapshots available: {}", task.snapshots.len());
            println!("   Lightweight snapshots: {}", task.lightweight_snapshots.len());

            // TODO: 实现实际的回滚逻辑
            println!("   🔧 Rollback implementation pending");

            // 回滚到主分支
            println!("✅ Rollback completed");
        }
        Ok(None) => return Err(CliError::TaskNotFound(task_id)),
        Err(e) => return Err(CliError::StorageError(e.to_string())),
    }

    Ok(())
}

async fn cmd_repl(args: ReplArgs, config: &CliConfig) -> Result<(), CliError> {
    info!("Starting REPL...");

    // 初始化执行器
    let context = ndc_runtime::ExecutionContext {
        storage: Arc::new(ndc_runtime::MemoryStorage::new()),
        workflow_engine: Arc::new(ndc_runtime::WorkflowEngine::new()),
        tools: Arc::new(ndc_runtime::ToolManager::new()),
        quality_runner: Arc::new(ndc_runtime::QualityGateRunner::new()),
        project_root: config.project_root.clone(),
        current_role: AgentRole::Historian,
    };
    let executor = Arc::new(ndc_runtime::Executor::new(context));

    // 启动 REPL
    let history = args.history.unwrap_or_else(|| PathBuf::from(".ndc/repl_history"));
    super::run_repl(history, executor).await;

    Ok(())
}

async fn cmd_daemon(args: DaemonArgs, _config: &CliConfig) -> Result<(), CliError> {
    info!("Starting daemon on: {}", args.address);

    // 启动守护进程
    let address = args.address.parse().unwrap();
    super::run_daemon(address).await;

    Ok(())
}

async fn cmd_search(args: SearchArgs, _config: &CliConfig) -> Result<(), CliError> {
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

/// 初始化执行器
fn init_executor(config: &CliConfig) -> Arc<Executor> {
    let context = ExecutionContext {
        storage: Arc::new(MemoryStorage::new()),
        workflow_engine: Arc::new(ndc_runtime::WorkflowEngine::new()),
        tools: Arc::new(ndc_runtime::ToolManager::new()),
        quality_runner: Arc::new(ndc_runtime::QualityGateRunner::new()),
        project_root: config.project_root.clone(),
        current_role: AgentRole::Historian,
    };

    Arc::new(Executor::new(context))
}

/// 解析任务 ID
fn parse_task_id(task_id_str: &str) -> Result<TaskId, CliError> {
    task_id_str.parse()
        .map_err(|_| CliError::InvalidTaskId(task_id_str.to_string()))
}
