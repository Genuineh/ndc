//! Tools - Controlled toolset
//!
//! Responsibilities:
//! - Safe file operations
//! - Safe Git operations
//! - Safe Shell command execution
//! - All operations validated and logged
//!
//! Architecture:
//! - Tool trait: Unified interface for all tools
//! - ToolRegistry: Dynamic tool registration and management
//! - JSON Schema: LLM-friendly parameter definitions

mod trait_mod;
pub use trait_mod::{Tool, ToolResult, ToolError, ToolContext, ToolManager, ToolMetadata};

pub mod schema;
pub use schema::{
    JsonSchema,
    JsonSchemaProperty,
    ToolSchemaBuilder,
    SchemaValidator,
    ValidationResult,
    generate_tool_description,
};

pub mod registry;
pub use registry::{ToolRegistry, ToolMetadata as RegistryToolMetadata, RegistrySummary, PredefinedCategories};

pub mod fs;
pub use fs::FsTool;

pub mod git;
pub use git::GitTool;

pub mod shell;
pub use shell::ShellTool;

// P4.2 Core Tools
pub mod list_tool;
pub use list_tool::ListTool;

pub mod read_tool;
pub use read_tool::ReadTool;

pub mod write_tool;
pub use write_tool::WriteTool;

pub mod edit_tool;
pub use edit_tool::EditTool;

pub mod grep_tool;
pub use grep_tool::GrepTool;

pub mod glob_tool;
pub use glob_tool::GlobTool;

pub mod permission;
pub use permission::{PermissionSystem, PermissionRequest, PermissionResponse, PermissionConfig, PermissionError, PermissionType, DangerLevel, PermissionSystemBuilder};

pub mod output_truncation;
pub use output_truncation::{OutputTruncator, TruncatedOutput, TruncationConfig, read_partial_output};

pub mod lsp;
pub use lsp::{LspClient, LspDiagnostics, Diagnostic, DiagnosticSeverity, DiagnosticSummary};

pub mod webfetch;
pub use webfetch::WebFetchTool;

pub mod websearch;
pub use websearch::WebSearchTool;

// P7 NDC Task Tools (AI-callable tools)
pub mod ndc;
pub use ndc::{
    TaskCreateTool,
    TaskUpdateTool,
    TaskListTool,
    TaskVerifyTool,
};

// P6 File Locking
pub mod locking;
pub use locking::{
    FileLockManager,
    FileLock,
    LockOwner,
    LockType,
    LockError,
    LockRequest,
    LockResult,
    EditToolWithLocking,
};

// P4.3 Bash Parsing
pub mod bash_parsing;
pub use bash_parsing::{
    BashParser,
    BashPermissionRequest,
    ParsedBashCommand,
    CommandType,
    FileOperation,
    FileOpType,
    BashDangerLevel,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;
    use std::path::PathBuf;

    // ===== FsTool Tests =====

    #[tokio::test]
    async fn test_fs_tool_new() {
        let tool = FsTool::new();
        assert_eq!(tool.name(), "fs");
        assert!(tool.description().contains("read"));
    }

    #[tokio::test]
    async fn test_fs_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"Hello, World!").unwrap();

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "read",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Hello, World!"));
        assert_eq!(result.metadata.files_read, 1);
    }

    #[tokio::test]
    async fn test_fs_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "write",
            "path": file_path.to_string_lossy(),
            "content": "Test content"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("output.txt"));
        assert_eq!(result.metadata.files_written, 1);

        // Verify file was written
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Test content");
    }

    #[tokio::test]
    async fn test_fs_create_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "create",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_create_directory() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("new_dir");

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "create",
            "path": dir_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_fs_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");
        File::create(&file_path).unwrap();

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "delete",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_fs_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        let _ = File::create(temp_dir.path().join("file1.txt")).unwrap();
        let _ = File::create(temp_dir.path().join("file2.txt")).unwrap();

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "list",
            "path": temp_dir.path().to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
    }

    #[tokio::test]
    async fn test_fs_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("exists.txt");
        File::create(&file_path).unwrap();

        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "exists",
            "path": file_path.to_string_lossy()
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "true");

        // Test non-existent file
        let params = serde_json::json!({
            "operation": "exists",
            "path": temp_dir.path().join("not_exists.txt").to_string_lossy()
        });
        let result = tool.execute(&params).await.unwrap();
        assert_eq!(result.output, "false");
    }

    #[tokio::test]
    async fn test_fs_invalid_operation() {
        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "invalid_op",
            "path": "/some/path"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
        match result {
            Err(ToolError::InvalidArgument(msg)) => {
                assert!(msg.contains("Unknown operation"));
            }
            _ => panic!("Expected InvalidArgument error"),
        }
    }

    #[tokio::test]
    async fn test_fs_missing_path() {
        let tool = FsTool::new();
        let params = serde_json::json!({
            "operation": "read"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    // ===== ShellTool Tests =====

    #[tokio::test]
    async fn test_shell_tool_new() {
        let tool = ShellTool::new();
        assert_eq!(tool.name(), "shell");
    }

    #[tokio::test]
    async fn test_shell_echo() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "echo",
            "args": ["hello", "world"]
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("hello world"));
    }

    #[tokio::test]
    async fn test_shell_pwd() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "pwd"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        // Should return current directory
        let current_dir = std::env::current_dir().unwrap();
        assert!(result.output.contains(&*current_dir.to_string_lossy()));
    }

    #[tokio::test]
    async fn test_shell_ls() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "ls",
            "args": ["-1"]
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_shell_blocked_command() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "rm",
            "args": ["-rf", "/"]
        });

        let result = tool.execute(&params).await;
        match result {
            Err(ToolError::PermissionDenied(msg)) => {
                assert!(msg.contains("not allowed"));
            }
            _ => panic!("Expected PermissionDenied error"),
        }
    }

    #[tokio::test]
    async fn test_shell_cat_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "cat test content").unwrap();

        let tool = ShellTool::new();
        let params = serde_json::json!({
            "command": "cat",
            "args": [file_path.to_string_lossy()]
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("cat test content"));
    }

    #[tokio::test]
    async fn test_shell_missing_command() {
        let tool = ShellTool::new();
        let params = serde_json::json!({
            "args": ["test"]
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_failed_command() {
        let tool = ShellTool::new();
        // cat a non-existent file will fail
        let params = serde_json::json!({
            "command": "cat",
            "args": ["/path/does/not/exist"]
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    // ===== GitTool Tests =====

    #[tokio::test]
    async fn test_git_tool_new() {
        let tool = GitTool::new();
        assert_eq!(tool.name(), "git");
    }

    #[tokio::test]
    async fn test_git_status() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "status"
        });

        let result = tool.execute(&params).await.unwrap();
        // Result may be empty if no changes, but should succeed
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_git_branch() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "branch"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        // Should contain branch info
    }

    #[tokio::test]
    async fn test_git_branch_current() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "branch_current"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
        // Should return current branch name
    }

    #[tokio::test]
    async fn test_git_log() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "log"
        });

        let result = tool.execute(&params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_git_remote() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "remote"
        });

        let result = tool.execute(&params).await.unwrap();
        // May be empty if no remotes
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_git_invalid_operation() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "operation": "invalid_op"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_missing_operation() {
        let tool = GitTool::new();
        let params = serde_json::json!({
            "message": "test"
        });

        let result = tool.execute(&params).await;
        assert!(result.is_err());
    }

    // ===== ToolManager Tests =====

    #[tokio::test]
    async fn test_tool_manager_new() {
        let manager = ToolManager::new();
        // ToolManager created successfully
        let tool = manager.get("nonexistent");
        assert!(tool.is_none());
    }

    #[tokio::test]
    async fn test_tool_manager_register() {
        let mut manager = ToolManager::new();
        let fs_tool = FsTool::new();
        manager.register("fs", fs_tool);

        let tool = manager.get("fs");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "fs");
    }

    #[tokio::test]
    async fn test_tool_manager_execute() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "manager test").unwrap();

        let mut manager = ToolManager::new();
        let fs_tool = FsTool::new();
        manager.register("fs", fs_tool);

        let params = serde_json::json!({
            "operation": "read",
            "path": file_path.to_string_lossy()
        });

        let result = manager.execute("fs", &params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("manager test"));
    }

    #[tokio::test]
    async fn test_tool_manager_not_found() {
        let manager = ToolManager::new();
        let params = serde_json::json!({});

        let result = manager.execute("nonexistent", &params).await;
        match result {
            Err(ToolError::NotFound(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    // ===== ToolContext Tests =====

    #[test]
    fn test_tool_context_default() {
        let context = ToolContext::default();
        assert!(context.working_dir.exists() || context.working_dir.to_string_lossy() == ".");
        assert!(!context.read_only);
        assert!(context.timeout_seconds > 0);
    }

    #[test]
    fn test_tool_context_custom() {
        let context = ToolContext {
            working_dir: PathBuf::from("/tmp"),
            env_vars: std::collections::HashMap::new(),
            allowed_operations: vec!["test".to_string()],
            read_only: true,
            timeout_seconds: 60,
        };

        assert_eq!(context.working_dir, PathBuf::from("/tmp"));
        assert!(context.read_only);
        assert_eq!(context.timeout_seconds, 60);
    }

    // ===== ToolResult Tests =====

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult {
            success: true,
            output: "test output".to_string(),
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: 100,
                files_read: 1,
                files_written: 0,
                bytes_processed: 50,
            },
        };

        assert!(result.success);
        assert_eq!(result.output, "test output");
        assert!(result.error.is_none());
        assert_eq!(result.metadata.execution_time_ms, 100);
    }

    #[test]
    fn test_tool_result_failure() {
        let result = ToolResult {
            success: false,
            output: "".to_string(),
            error: Some("error message".to_string()),
            metadata: ToolMetadata {
                execution_time_ms: 50,
                files_read: 0,
                files_written: 0,
                bytes_processed: 0,
            },
        };

        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.error.unwrap(), "error message");
    }

    // ===== ToolSchema Tests =====

    #[tokio::test]
    async fn test_fs_tool_schema() {
        let tool = FsTool::new();
        let schema = tool.schema();
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert!(obj.contains_key("properties"));
        assert!(obj.get("required").unwrap().is_array());
    }

    #[tokio::test]
    async fn test_shell_tool_schema() {
        let tool = ShellTool::new();
        let schema = tool.schema();
        assert!(schema.is_object());
    }

    #[tokio::test]
    async fn test_git_tool_schema() {
        let tool = GitTool::new();
        let schema = tool.schema();
        assert!(schema.is_object());
    }
}
