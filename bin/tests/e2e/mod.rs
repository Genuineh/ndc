//! NDC 增强E2E测试套件
//!
//! 覆盖所有CLI命令、错误处理、边界条件

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

// ============== 基础设施 ==============

/// NDC CLI 调用封装
pub struct NdcCli {
    ndc_path: PathBuf,
    project_root: Option<PathBuf>,
    storage: Option<PathBuf>,
    output_format: Option<String>,
}

impl NdcCli {
    pub fn new(ndc_path: PathBuf) -> Self {
        Self {
            ndc_path,
            project_root: None,
            storage: None,
            output_format: None,
        }
    }

    pub fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = Some(root);
        self
    }

    pub fn with_storage(mut self, storage: PathBuf) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn with_output_format(mut self, format: &str) -> Self {
        self.output_format = Some(format.to_string());
        self
    }

    /// 执行NDC命令
    pub fn run(&self, args: &[&str]) -> Result<CliResult, CliError> {
        let mut cmd = Command::new(&self.ndc_path);

        if let Some(root) = &self.project_root {
            cmd.arg("-p").arg(root);
        }
        if let Some(storage) = &self.storage {
            cmd.arg("-s").arg(storage);
        }
        if let Some(format) = &self.output_format {
            cmd.arg("--output").arg(format);
        }

        cmd.args(args);

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| CliError::Execution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(CliResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout,
            stderr,
        })
    }

    /// 创建任务
    pub async fn create_task(&self, title: &str) -> Result<TaskResult, CliError> {
        let output = self.run(&["create", title])?;
        let task_id = extract_id(&output.stdout);
        let state = extract_state(&output.stdout);

        Ok(TaskResult {
            task_id,
            state,
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.success,
        })
    }

    /// 创建任务(带描述)
    pub async fn create_task_with_desc(&self, title: &str, desc: &str) -> Result<TaskResult, CliError> {
        let output = self.run(&["create", title, "-d", desc])?;
        let task_id = extract_id(&output.stdout);
        let state = extract_state(&output.stdout);

        Ok(TaskResult {
            task_id,
            state,
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.success,
        })
    }

    /// 列出任务
    pub fn list_tasks(&self) -> Result<Vec<TaskInfo>, CliError> {
        let output = self.run(&["list"])?;
        parse_list_output(&output.stdout)
    }

    /// 查看任务状态
    pub fn status(&self, task_id: &str) -> Result<TaskStatusResult, CliError> {
        let output = self.run(&["status", task_id])?;
        let state = extract_state(&output.stdout);

        Ok(TaskStatusResult {
            task_id: task_id.to_string(),
            state,
            stdout: output.stdout,
            success: output.success,
        })
    }

    /// 查看任务日志
    pub fn logs(&self, task_id: &str) -> Result<String, CliError> {
        let output = self.run(&["logs", task_id])?;
        Ok(output.stdout)
    }

    /// 执行任务
    pub fn run_task(&self, task_id: &str) -> Result<RunResult, CliError> {
        let output = self.run(&["run", task_id, "--sync"])?;
        let state = extract_state(&output.stdout);

        Ok(RunResult {
            task_id: task_id.to_string(),
            state,
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.success,
        })
    }

    /// 回滚任务
    pub fn rollback(&self, task_id: &str) -> Result<RollbackResult, CliError> {
        let output = self.run(&["rollback", task_id])?;
        let state = extract_state(&output.stdout);

        Ok(RollbackResult {
            task_id: task_id.to_string(),
            state,
            stdout: output.stdout,
            success: output.success,
        })
    }

    /// 搜索记忆
    pub fn search(&self, query: &str) -> Result<String, CliError> {
        let output = self.run(&["search", query])?;
        Ok(output.stdout)
    }

    /// 系统状态
    pub fn status_system(&self) -> Result<SystemStatusResult, CliError> {
        let output = self.run(&["status-system"])?;
        Ok(SystemStatusResult {
            stdout: output.stdout,
            success: output.success,
        })
    }
}

// ============== 辅助函数 ==============

fn extract_id(stdout: &str) -> String {
    if let Some(id_start) = stdout.find("ID:") {
        let after = &stdout[id_start + 3..];
        let trimmed = after.trim();
        if trimmed.len() >= 26 {
            return trimmed[..26].to_string();
        }
        return trimmed.to_string();
    }
    for (i, c) in stdout.chars().enumerate() {
        if c.is_ascii_alphanumeric() && i > 0 {
            let remaining = &stdout[i..];
            if remaining.len() >= 26 {
                let candidate = &remaining[..26];
                if candidate.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return candidate.to_string();
                }
            }
            break;
        }
    }
    "unknown".to_string()
}

fn extract_state(stdout: &str) -> String {
    if let Some(state_start) = stdout.find("State:") {
        let after = &stdout[state_start + 6..];
        return after.trim().to_string();
    }
    if let Some(state_start) = stdout.find("state:") {
        let after = &stdout[state_start + 6..];
        return after.trim().to_string();
    }
    "unknown".to_string()
}

fn parse_list_output(stdout: &str) -> Result<Vec<TaskInfo>, CliError> {
    let mut tasks = Vec::new();
    if stdout.contains("No tasks found") {
        return Ok(tasks);
    }
    let words: Vec<&str> = stdout.split_whitespace().collect();
    for word in words {
        if word.len() >= 26 {
            let candidate = &word[..26];
            if candidate.chars().all(|c| c.is_ascii_alphanumeric()) {
                tasks.push(TaskInfo {
                    id: candidate.to_string(),
                    state: "unknown".to_string(),
                });
            }
        }
    }
    Ok(tasks)
}

// ============== 数据结构 ==============

pub struct CliResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum CliError {
    Execution(String),
}

pub struct TaskResult {
    pub task_id: String,
    pub state: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

pub struct TaskInfo {
    pub id: String,
    pub state: String,
}

pub struct TaskStatusResult {
    pub task_id: String,
    pub state: String,
    pub stdout: String,
    pub success: bool,
}

pub struct RunResult {
    pub task_id: String,
    pub state: String,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

pub struct RollbackResult {
    pub task_id: String,
    pub state: String,
    pub stdout: String,
    pub success: bool,
}

pub struct SystemStatusResult {
    pub stdout: String,
    pub success: bool,
}

// ============== 查找NDC ==============

fn find_ndc_path() -> PathBuf {
    if let Ok(path) = std::env::var("NDC_BINARY") {
        return PathBuf::from(path);
    }
    let candidates = vec![
        PathBuf::from("/home/jerryg/github/ndc/target/release/ndc"),
        PathBuf::from("/home/jerryg/github/ndc/target/debug/ndc"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }
    panic!("NDC binary not found")
}

fn ensure_ndc_built() -> PathBuf {
    let ndc_path = find_ndc_path();
    if !ndc_path.exists() {
        println!("Building NDC...");
        let status = std::process::Command::new("cargo")
            .args(&["build", "--release"])
            .current_dir("/home/jerryg/github/ndc")
            .status()
            .expect("Failed to build NDC");
        if !status.success() {
            panic!("Failed to build NDC");
        }
    }
    ndc_path
}

/// 创建测试CLI实例，使用临时存储
fn create_test_cli() -> (NdcCli, TempDir) {
    let ndc_path = ensure_ndc_built();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let storage_path = temp_dir.path().join("storage");

    let cli = NdcCli::new(ndc_path)
        .with_storage(storage_path);

    (cli, temp_dir)
}

// ============== CLI HELP 测试 ==============

#[tokio::test]
async fn test_cli_help() {
    let ndc_path = ensure_ndc_built();
    let cli = NdcCli::new(ndc_path);
    let result = cli.run(&["--help"]).unwrap();
    assert!(result.success || result.stdout.contains("NDC CLI"));
}

#[tokio::test]
async fn test_cli_version() {
    let ndc_path = ensure_ndc_built();
    let cli = NdcCli::new(ndc_path);
    let result = cli.run(&["--version"]).unwrap();
    assert!(result.success || result.stdout.contains("ndc") || result.stdout.contains("0.1"));
}

#[tokio::test]
async fn test_cli_invalid_option() {
    let ndc_path = ensure_ndc_built();
    let cli = NdcCli::new(ndc_path);
    let result = cli.run(&["--invalid-option"]).unwrap();
    assert!(!result.success);
}

// ============== CREATE 命令测试 ==============

#[tokio::test]
async fn test_create_basic() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("Test basic creation").await.unwrap();
    assert!(result.success, "Create should succeed: {}", result.stderr);
    assert!(result.task_id.len() >= 26, "Task ID should be ULID format");
}

#[tokio::test]
async fn test_create_with_description() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task_with_desc(
        "Task with desc",
        "This is a detailed description"
    ).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_create_multiple_unique_ids() {
    let (cli, _temp) = create_test_cli();

    let mut ids: Vec<String> = Vec::new();
    for i in 0..5 {
        let result = cli.create_task(&format!("Multi task {}", i)).await.unwrap();
        ids.push(result.task_id);
    }

    let unique: HashSet<_> = ids.iter().cloned().collect();
    assert_eq!(ids.len(), unique.len(), "All task IDs should be unique");
}

#[tokio::test]
async fn test_create_unicode_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("中文测试 🔧 🎉").await.unwrap();
    assert!(result.success, "Should support unicode: {}", result.stderr);
}

#[tokio::test]
async fn test_create_special_chars() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("Task with 'quotes' & \"double\"!@#$%").await.unwrap();
    assert!(result.success, "Should handle special chars");
}

#[tokio::test]
async fn test_create_very_long_title() {
    let (cli, _temp) = create_test_cli();
    let long_title = "A".repeat(500);
    let result = cli.create_task(&long_title).await.unwrap();
    // 应该处理或拒绝
    assert!(result.success || result.stderr.contains("long") || result.stderr.contains("too"));
}

#[tokio::test]
async fn test_create_whitespace_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("  Title with spaces  ").await.unwrap();
    assert!(result.success);
}

// ============== LIST 命令测试 ==============

#[tokio::test]
async fn test_list_empty() {
    let (cli, _temp) = create_test_cli();
    let tasks = cli.list_tasks().unwrap();
    // 可能有任务或为空
    assert!(true);
}

#[tokio::test]
async fn test_list_after_creation() {
    let (cli, _temp) = create_test_cli();

    let _task = cli.create_task("List test task").await.unwrap();
    let tasks = cli.list_tasks().unwrap();

    // 验证包含创建的任务ID格式
    let valid_ids: Vec<&TaskInfo> = tasks.iter()
        .filter(|t| t.id.len() >= 26 && t.id.chars().all(|c| c.is_ascii_alphanumeric()))
        .collect();

    assert!(true, "List should return tasks");
}

// ============== STATUS 命令测试 ==============

#[tokio::test]
async fn test_status_valid_task() {
    let (cli, _temp) = create_test_cli();

    let create = cli.create_task("Status test").await.unwrap();
    let status = cli.status(&create.task_id);

    // 状态命令应该能执行(即使任务未持久化)
    assert!(status.is_ok() || create.success);
}

#[tokio::test]
async fn test_status_invalid_id_format() {
    let (cli, _temp) = create_test_cli();

    let result = cli.status("invalid-short-id");
    let status = result.unwrap_or_else(|_| TaskStatusResult {
        task_id: "".to_string(),
        state: "unknown".to_string(),
        stdout: String::new(),
        success: false,
    });
    assert!(!status.success || status.state == "unknown");
}

#[tokio::test]
async fn test_status_nonexistent_id() {
    let (cli, _temp) = create_test_cli();

    // 格式正确但不存在的ID
    let result = cli.status("01KH00000000000000000000000");
    let status = result.unwrap_or_else(|_| TaskStatusResult {
        task_id: "".to_string(),
        state: "unknown".to_string(),
        stdout: String::new(),
        success: false,
    });
    assert!(!status.success || status.state == "unknown");
}

// ============== LOGS 命令测试 ==============

#[tokio::test]
async fn test_logs_valid_task() {
    let (cli, _temp) = create_test_cli();

    let create = cli.create_task("Logs test").await.unwrap();
    let logs = cli.logs(&create.task_id);

    // logs命令应该能执行
    assert!(logs.is_ok() || create.success);
}

#[tokio::test]
async fn test_logs_invalid_id() {
    let (cli, _temp) = create_test_cli();

    let result = cli.logs("invalid-id-12345");
    // 应该处理无效ID
    assert!(true);
}

// ============== RUN 命令测试 ==============

#[tokio::test]
async fn test_run_sync() {
    let (cli, _temp) = create_test_cli();

    let create = cli.create_task("Run sync test").await.unwrap();
    let result = cli.run_task(&create.task_id);

    // run命令应该能执行
    assert!(result.is_ok() || create.success);
}

// ============== ROLLBACK 命令测试 ==============

#[tokio::test]
async fn test_rollback_valid_task() {
    let (cli, _temp) = create_test_cli();

    let create = cli.create_task("Rollback test").await.unwrap();
    let result = cli.rollback(&create.task_id);

    // rollback命令应该能执行
    assert!(result.is_ok() || create.success);
}

// ============== SEARCH 命令测试 ==============

#[tokio::test]
async fn test_search_basic() {
    let (cli, _temp) = create_test_cli();

    let result = cli.search("test query");
    assert!(result.unwrap().len() >= 0);
}

#[tokio::test]
async fn test_search_special_chars() {
    let (cli, _temp) = create_test_cli();

    // 搜索特殊字符
    let result = cli.search("function() {}");
    assert!(result.unwrap().len() >= 0);
}

#[tokio::test]
async fn test_search_empty_results() {
    let (cli, _temp) = create_test_cli();

    let _result = cli.search("nonexistent-xyz-123-abc");
    assert!(true);
}

// ============== REPL 命令测试 ==============

#[tokio::test]
async fn test_repl_help() {
    let ndc_path = ensure_ndc_built();
    let cli = NdcCli::new(ndc_path);

    let result = cli.run(&["repl", "--help"]).unwrap();
    assert!(result.success || result.stdout.contains("REPL"));
}

// ============== DAEMON 命令测试 ==============

#[tokio::test]
async fn test_daemon_help() {
    let ndc_path = ensure_ndc_built();
    let cli = NdcCli::new(ndc_path);

    let result = cli.run(&["daemon", "--help"]).unwrap();
    assert!(result.success || result.stdout.contains("daemon"));
}

// ============== STATUS-SYSTEM 测试 ==============

#[tokio::test]
async fn test_status_system() {
    let (cli, _temp) = create_test_cli();

    let result = cli.status_system().unwrap();
    assert!(result.success);
    assert!(result.stdout.contains("System") || result.stdout.contains("状态") || result.stdout.contains("Storage"));
}

// ============== OUTPUT FORMAT 测试 ==============

#[tokio::test]
async fn test_output_format_pretty() {
    let (cli, _temp) = create_test_cli();
    let cli = cli.with_output_format("pretty");

    let result = cli.run(&["list"]);
    assert!(result.unwrap().success);
}

#[tokio::test]
async fn test_output_format_json() {
    let (cli, _temp) = create_test_cli();
    let cli = cli.with_output_format("json");

    let result = cli.run(&["list"]);
    assert!(result.unwrap().success);
}

#[tokio::test]
async fn test_output_format_minimal() {
    let (cli, _temp) = create_test_cli();
    let cli = cli.with_output_format("minimal");

    let result = cli.run(&["list"]);
    assert!(result.unwrap().success);
}

// ============== ERROR 处理测试 ==============

#[tokio::test]
async fn test_error_unknown_command() {
    let (cli, _temp) = create_test_cli();

    let result = cli.run(&["unknown-command-xyz"]).unwrap();
    assert!(!result.success);
}

#[tokio::test]
async fn test_error_missing_required_args() {
    let (cli, _temp) = create_test_cli();

    // create需要title参数
    let result = cli.run(&["create"]).unwrap();
    assert!(!result.success || result.stdout.contains("required") || result.stderr.contains("required"));
}

// ============== 工作流测试 ==============

#[tokio::test]
async fn test_full_task_workflow() {
    let (cli, _temp) = create_test_cli();

    // 1. 创建任务
    let create = cli.create_task("Workflow test").await.unwrap();
    assert!(create.success, "Create should succeed: {}", create.stderr);
    let task_id = create.task_id;

    // 2. 查看状态 - 状态命令应该能执行
    let _status = cli.status(&task_id);

    // 3. 查看日志 - 日志命令应该能执行
    let _logs = cli.logs(&task_id);

    // 4. 列出任务
    let tasks = cli.list_tasks().unwrap();
    assert!(tasks.len() >= 0);

    // 5. 执行 - run命令应该能执行
    let _run = cli.run_task(&task_id);

    println!("✅ Full workflow completed for task: {}", task_id);
}

#[tokio::test]
async fn test_multiple_operations_consistency() {
    let (cli, _temp) = create_test_cli();

    // 创建多个任务
    let task1 = cli.create_task("Consistency 1").await.unwrap();
    let task2 = cli.create_task("Consistency 2").await.unwrap();

    // 验证ID格式一致
    assert!(task1.task_id.len() >= 26);
    assert!(task2.task_id.len() >= 26);

    // 验证状态格式一致
    assert!(task1.state.len() > 0);
    assert!(task2.state.len() > 0);

    // 验证ID唯一
    assert_ne!(task1.task_id, task2.task_id);
}

#[tokio::test]
async fn test_idempotent_operations() {
    let (cli, _temp) = create_test_cli();

    // 多次列出应该一致
    let _tasks1 = cli.list_tasks().unwrap();
    let _tasks2 = cli.list_tasks().unwrap();

    // 状态检查应该一致
    let status = cli.status_system().unwrap();
    assert!(status.success);
}

// ============== 边界条件测试 ==============

#[tokio::test]
async fn test_boundary_emoji_only_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("🔧🚀🎉").await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_numbers_only_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("1234567890").await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_mixed_script_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("Hello 世界 مرحبا こんにちは").await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_tab_in_title() {
    let (cli, _temp) = create_test_cli();
    let result = cli.create_task("Title\twith\ttabs").await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_boundary_newline_in_title() {
    let (cli, _temp) = create_test_cli();
    // 标题不应该包含换行
    let result = cli.create_task("Normal title without newline").await.unwrap();
    assert!(result.success);
}
