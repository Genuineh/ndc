//! Bash Command Parser - Parse bash commands
//!
//! Responsibilities:
//! - Parse bash commands
//! - Extract file operations from commands
//! - Detect dangerous patterns
//! - Auto-request permissions for file operations

use std::path::PathBuf;
use std::sync::Arc;

/// Parsed bash command
#[derive(Debug, Clone)]
pub struct ParsedBashCommand {
    /// The raw command
    pub command: String,
    /// Command type
    pub command_type: CommandType,
    /// File operations detected
    pub file_operations: Vec<FileOperation>,
    /// Danger level
    pub danger_level: BashDangerLevel,
    /// Arguments extracted
    pub arguments: Vec<String>,
    /// Working directory if specified
    pub working_dir: Option<PathBuf>,
}

/// Type of bash command
#[derive(Debug, Clone, PartialEq)]
pub enum CommandType {
    /// Simple command
    Simple,
    /// Piped command
    Piped,
    /// Compound command with && or ||
    Compound,
    /// Redirection
    Redirect,
    /// Control flow (if/while/for)
    ControlFlow,
    /// Unknown
    Unknown,
}

/// A file operation detected in a command
#[derive(Debug, Clone)]
pub struct FileOperation {
    /// Type of operation
    pub operation_type: FileOpType,
    /// Path involved
    pub path: PathBuf,
    /// Whether path is a glob pattern
    pub is_pattern: bool,
}

/// Type of file operation
#[derive(Debug, Clone, PartialEq)]
pub enum FileOpType {
    /// Read operation
    Read,
    /// Write operation
    Write,
    /// Create operation
    Create,
    /// Delete operation
    Delete,
    /// Execute/Run operation
    Execute,
    /// Move/Rename operation
    Move,
    /// Chmod operation
    Chmod,
    /// Chown operation
    Chown,
    /// Unknown
    Unknown,
}

/// Danger level
#[derive(Debug, Clone, PartialEq)]
pub enum BashDangerLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl BashDangerLevel {
    pub fn is_safe(&self) -> bool {
        *self == BashDangerLevel::Safe
    }

    pub fn needs_confirmation(&self) -> bool {
        *self == BashDangerLevel::High || *self == BashDangerLevel::Critical
    }
}

/// Bash parser
#[derive(Debug, Clone)]
pub struct BashParser {
    /// Configured allowed operations
    allowed_ops: Arc<Vec<String>>,
}

impl BashParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self {
            allowed_ops: Arc::new(vec![]),
        }
    }

    /// Create with allowed operations
    pub fn with_allowed_ops(allowed_ops: Vec<String>) -> Self {
        Self {
            allowed_ops: Arc::new(allowed_ops),
        }
    }

    /// Set allowed operations
    pub fn set_allowed_ops(&mut self, ops: Vec<String>) {
        self.allowed_ops = Arc::new(ops);
    }

    /// Parse a command string
    pub fn parse(&self, command: &str) -> Result<ParsedBashCommand, String> {
        // Extract arguments
        let arguments = Self::extract_arguments(command);

        // Detect command type
        let command_type = Self::detect_command_type(command);

        // Detect file operations
        let file_operations = Self::detect_file_operations(&arguments);

        // Check danger level
        let danger_level = Self::assess_danger(command, &file_operations);

        // Detect working directory changes
        let working_dir = Self::detect_working_dir(&arguments);

        Ok(ParsedBashCommand {
            command: command.to_string(),
            command_type,
            file_operations,
            danger_level,
            arguments,
            working_dir,
        })
    }

    fn detect_command_type(command: &str) -> CommandType {
        let trimmed = command.trim();

        if trimmed.contains(" | ") {
            return CommandType::Piped;
        }

        if trimmed.contains(" && ") || trimmed.contains(" || ") {
            return CommandType::Compound;
        }

        if trimmed.contains(" > ") || trimmed.contains(" >> ") || trimmed.contains(" < ") {
            return CommandType::Redirect;
        }

        if trimmed.starts_with("if ")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("while ")
        {
            return CommandType::ControlFlow;
        }

        CommandType::Simple
    }

    fn extract_arguments(command: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let chars = command.chars().peekable();

        for c in chars {
            if c == '"' || c == '\'' {
                in_quotes = !in_quotes;
                continue;
            }

            if in_quotes {
                current.push(c);
            } else if c == ' ' {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            } else {
                current.push(c);
            }
        }

        if !current.is_empty() {
            args.push(current);
        }

        args
    }

    fn detect_file_operations(arguments: &[String]) -> Vec<FileOperation> {
        let mut ops = Vec::new();

        // Common file operation commands
        let read_cmds = [
            "cat", "less", "more", "head", "tail", "grep", "rg", "find", "wc", "sort", "uniq",
        ];
        let write_cmds = ["echo", "printf", "tee", "sed", "awk"];
        let delete_cmds = ["rm", "del", "unlink", "rmdir"];
        let execute_cmds = [
            ".", "source", "bash", "sh", "zsh", "python", "python3", "node", "cargo", "make",
        ];
        let move_cmds = ["mv", "rename", "cp", "rsync"];
        let chmod_cmds = ["chmod", "chown", "chgrp"];

        if arguments.is_empty() {
            return ops;
        }

        let cmd = arguments[0].to_lowercase();

        let op_type = if read_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Read
        } else if write_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Write
        } else if delete_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Delete
        } else if execute_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Execute
        } else if move_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Move
        } else if chmod_cmds.iter().any(|c| cmd == *c) {
            FileOpType::Chmod
        } else if cmd == "touch" || cmd == "mkdir" {
            FileOpType::Create
        } else {
            FileOpType::Unknown
        };

        // Extract paths from arguments (skip command name)
        for arg in &arguments[1..] {
            // Check if it looks like a path
            let starts_with_slash = arg.starts_with('/');
            let starts_with_dot_slash = arg.starts_with("./");
            let starts_with_dot_dot_slash = arg.starts_with("../");
            let has_glob = arg.contains('*') || arg.contains('?');
            let has_extension = arg.len() > 1
                && arg.contains('.')
                && arg
                    .chars()
                    .next()
                    .map(|c| c.is_alphabetic())
                    .unwrap_or(false);

            if starts_with_slash
                || starts_with_dot_slash
                || starts_with_dot_dot_slash
                || has_glob
                || has_extension
            {
                let is_pattern = has_glob;
                ops.push(FileOperation {
                    operation_type: op_type.clone(),
                    path: PathBuf::from(arg),
                    is_pattern,
                });
            }
        }

        // For execute commands, also check subsequent arguments for scripts
        if op_type == FileOpType::Execute && arguments.len() > 1 {
            for arg in &arguments[1..] {
                if arg.ends_with(".sh")
                    || arg.ends_with(".js")
                    || arg.ends_with(".py")
                    || arg.ends_with(".rb")
                    || arg.ends_with(".php")
                {
                    ops.push(FileOperation {
                        operation_type: op_type.clone(),
                        path: PathBuf::from(arg),
                        is_pattern: false,
                    });
                    break;
                }
            }
        }

        ops
    }

    fn detect_working_dir(arguments: &[String]) -> Option<PathBuf> {
        if arguments.is_empty() {
            return None;
        }

        let cmd = arguments[0].to_lowercase();
        if cmd != "cd" && cmd != "chdir" && cmd != "pushd" {
            return None;
        }

        // Second argument is the path
        if arguments.len() > 1 {
            Some(PathBuf::from(&arguments[1]))
        } else {
            None
        }
    }

    fn assess_danger(command: &str, file_ops: &[FileOperation]) -> BashDangerLevel {
        let lower = command.to_lowercase();

        // Critical danger patterns (check first)
        let critical_patterns = [
            "rm -rf /",
            "rm -rf /usr",
            "rm -rf /bin",
            "rm -rf /etc",
            "mkfs",
            "dd if=/dev/zero",
        ];

        for pattern in &critical_patterns {
            if lower.contains(pattern) {
                return BashDangerLevel::Critical;
            }
        }

        // High danger patterns
        let high_patterns = [
            "rm -rf",
            "chmod -R 777",
            "> /dev/sda",
            "> /dev/sdb",
            "dd if=",
        ];

        for pattern in &high_patterns {
            if lower.contains(pattern) {
                return BashDangerLevel::High;
            }
        }

        // Check for chmod 777 pattern (after rm -rf since rm -rf is more dangerous)
        if lower.contains("chmod") && (lower.contains("777") || lower.contains("chmod 0")) {
            return BashDangerLevel::High;
        }

        // Medium danger: delete operations on real paths
        for op in file_ops {
            if op.operation_type == FileOpType::Delete {
                let path_str = op.path.to_string_lossy();
                if path_str.starts_with("/")
                    && !path_str.starts_with("/tmp")
                    && !path_str.starts_with("/var/tmp")
                {
                    return BashDangerLevel::Medium;
                }
            }
        }

        // Low danger: write operations
        for op in file_ops {
            if op.operation_type == FileOpType::Write
                || op.operation_type == FileOpType::Create
                || op.operation_type == FileOpType::Chmod
            {
                return BashDangerLevel::Low;
            }
        }

        BashDangerLevel::Safe
    }

    /// Extract patterns for permission request
    pub fn extract_permission_patterns(&self, command: &str) -> Vec<String> {
        let parsed = self.parse(command).ok();
        let mut patterns = Vec::new();

        if let Some(cmd) = parsed {
            // Add command name
            if let Some(first_arg) = cmd.arguments.first() {
                patterns.push(first_arg.clone());
            }

            // Add file paths
            for op in &cmd.file_operations {
                patterns.push(op.path.to_string_lossy().to_string());
            }
        } else {
            // Fallback: extract words
            for word in command.split_whitespace() {
                if !word.is_empty() && !word.starts_with('-') {
                    patterns.push(word.to_string());
                }
            }
        }

        patterns
    }

    /// Generate permission request from command
    pub fn check_permission(&self, command: &str) -> BashPermissionRequest {
        let parsed = self.parse(command).unwrap_or_else(|_| ParsedBashCommand {
            command: command.to_string(),
            command_type: CommandType::Unknown,
            file_operations: Vec::new(),
            danger_level: BashDangerLevel::Safe,
            arguments: Vec::new(),
            working_dir: None,
        });

        let patterns = self.extract_permission_patterns(command);

        let auto_allow = self
            .allowed_ops
            .iter()
            .any(|op| parsed.arguments.first().map(|a| a == op).unwrap_or(false));

        BashPermissionRequest {
            command: command.to_string(),
            patterns,
            file_operations: parsed.file_operations,
            danger_level: parsed.danger_level,
            auto_allow,
        }
    }
}

impl Default for BashParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Permission request from bash parsing
#[derive(Debug, Clone)]
pub struct BashPermissionRequest {
    pub command: String,
    pub patterns: Vec<String>,
    pub file_operations: Vec<FileOperation>,
    pub danger_level: BashDangerLevel,
    pub auto_allow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_command() {
        let parser = BashParser::new();
        let result = parser.parse("echo hello world").unwrap();

        assert_eq!(result.command_type, CommandType::Simple);
        assert!(result.file_operations.is_empty());
        assert_eq!(result.danger_level, BashDangerLevel::Safe);
        assert_eq!(result.arguments, vec!["echo", "hello", "world"]);
    }

    #[test]
    fn test_parse_rm_command() {
        let parser = BashParser::new();
        let result = parser.parse("rm -rf /tmp/test").unwrap();

        assert!(!result.file_operations.is_empty());
        assert_eq!(result.file_operations[0].operation_type, FileOpType::Delete);
        assert_eq!(result.file_operations[0].path, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_parse_cat_command() {
        let parser = BashParser::new();
        let result = parser.parse("cat /etc/passwd").unwrap();

        assert!(!result.file_operations.is_empty());
        assert_eq!(result.file_operations[0].operation_type, FileOpType::Read);
        assert_eq!(result.file_operations[0].path, PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn test_danger_level_critical() {
        let parser = BashParser::new();
        let result = parser.parse("rm -rf /").unwrap();

        assert_eq!(result.danger_level, BashDangerLevel::Critical);
    }

    #[test]
    fn test_danger_level_high() {
        let parser = BashParser::new();
        let result = parser.parse("chmod -R 777 /home").unwrap();

        assert_eq!(result.danger_level, BashDangerLevel::High);
    }

    #[test]
    fn test_danger_level_medium() {
        let parser = BashParser::new();
        let result = parser.parse("rm /etc/passwd").unwrap();

        assert_eq!(result.danger_level, BashDangerLevel::Medium);
    }

    #[test]
    fn test_danger_level_safe() {
        let parser = BashParser::new();
        let result = parser.parse("ls -la").unwrap();

        assert_eq!(result.danger_level, BashDangerLevel::Safe);
    }

    #[test]
    fn test_extract_patterns() {
        let parser = BashParser::new();
        let patterns = parser.extract_permission_patterns("cat /etc/hosts");

        assert!(patterns.contains(&"cat".to_string()));
        assert!(patterns.contains(&"/etc/hosts".to_string()));
    }

    #[test]
    fn test_check_permission_safe() {
        let parser = BashParser::with_allowed_ops(vec!["ls".to_string()]);
        let request = parser.check_permission("ls -la");

        assert!(request.auto_allow);
        assert_eq!(request.danger_level, BashDangerLevel::Safe);
    }

    #[test]
    fn test_check_permission_dangerous() {
        let parser = BashParser::new();
        // Use a path without "/" to avoid triggering critical pattern
        let request = parser.check_permission("rm -rf tmp");

        assert!(!request.auto_allow);
        assert_eq!(request.danger_level, BashDangerLevel::High);
    }

    #[test]
    fn test_glob_pattern_detection() {
        let parser = BashParser::new();
        let result = parser.parse("rm *.tmp").unwrap();

        assert!(!result.file_operations.is_empty());
        assert!(result.file_operations[0].is_pattern);
    }

    #[test]
    fn test_cd_command() {
        let parser = BashParser::new();
        let result = parser.parse("cd /tmp").unwrap();

        assert_eq!(result.working_dir, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_piped_command() {
        let parser = BashParser::new();
        let result = parser.parse("cat file.txt | grep pattern").unwrap();

        assert_eq!(result.command_type, CommandType::Piped);
    }

    #[test]
    fn test_compound_command() {
        let parser = BashParser::new();
        let result = parser.parse("echo a && echo b").unwrap();

        assert_eq!(result.command_type, CommandType::Compound);
    }

    #[test]
    fn test_redirect_command() {
        let parser = BashParser::new();
        let result = parser.parse("echo hello > output.txt").unwrap();

        assert_eq!(result.command_type, CommandType::Redirect);
    }

    #[test]
    fn test_quoted_args() {
        let parser = BashParser::new();
        let result = parser.parse("echo \"hello world\"").unwrap();

        assert_eq!(result.arguments, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_execute_command() {
        let parser = BashParser::new();
        let result = parser.parse("python script.py").unwrap();

        assert!(!result.file_operations.is_empty());
        assert_eq!(
            result.file_operations[0].operation_type,
            FileOpType::Execute
        );
    }

    #[test]
    fn test_mkdir_command() {
        let parser = BashParser::new();
        let result = parser.parse("mkdir -p /tmp/new_dir").unwrap();

        assert!(!result.file_operations.is_empty());
        assert_eq!(result.file_operations[0].operation_type, FileOpType::Create);
    }

    #[test]
    fn test_cp_command() {
        let parser = BashParser::new();
        let result = parser.parse("cp /tmp/file1 /tmp/file2").unwrap();

        assert!(!result.file_operations.is_empty());
        assert_eq!(result.file_operations[0].operation_type, FileOpType::Move);
    }
}
