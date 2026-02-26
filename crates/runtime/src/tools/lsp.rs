//! LSP Diagnostics Integration
//!
//! Responsibilities:
//! - Run LSP diagnostics on files
//! - Parse diagnostic output
//! - Integrate with edit operations
//! - Provide diagnostic summaries

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// LSP diagnostic severity
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// A single diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Diagnostic message
    pub message: String,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// File path (relative or absolute)
    pub file: PathBuf,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
    /// Error code if available
    pub code: Option<String>,
}

/// LSP diagnostic summary
#[derive(Debug, Clone)]
pub struct DiagnosticSummary {
    /// Total number of diagnostics
    pub total_count: usize,
    /// Count by severity
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub hint_count: usize,
    /// List of all diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Whether there are any errors
    pub has_errors: bool,
}

/// LSP client wrapper
#[derive(Debug, Clone)]
pub struct LspClient {
    /// LSP server command
    server_command: Vec<String>,
    /// Project root
    root: PathBuf,
}

impl LspClient {
    /// Create a new LSP client
    pub fn new(server_command: Vec<String>, root: PathBuf) -> Self {
        Self {
            server_command,
            root,
        }
    }

    /// Check if an LSP server is available
    pub fn is_available(&self) -> bool {
        // Try to run the server with --version or check if command exists
        if self.server_command.is_empty() {
            return false;
        }

        let mut cmd = Command::new(&self.server_command[0]);
        if self.server_command.len() > 1 {
            cmd.args(&self.server_command[1..]);
        }
        // Just check if command exists, don't actually run server
        cmd.current_dir(&self.root);

        // This will fail if command doesn't exist, which is fine
        cmd.status().map(|s| s.success()).unwrap_or(false)
    }

    /// Get diagnostics for a file
    pub async fn get_diagnostics(&self, file_path: &PathBuf) -> Result<DiagnosticSummary, String> {
        // Check if we have an LSP server
        if !self.is_available() {
            return Ok(DiagnosticSummary {
                total_count: 0,
                error_count: 0,
                warning_count: 0,
                info_count: 0,
                hint_count: 0,
                diagnostics: Vec::new(),
                has_errors: false,
            });
        }

        // Try to run LSP diagnostics
        self.run_lsp_diagnostics(file_path).await
    }

    /// Run LSP diagnostics
    async fn run_lsp_diagnostics(&self, file_path: &PathBuf) -> Result<DiagnosticSummary, String> {
        // Different LSP servers have different ways to get diagnostics
        // Try rust-analyzer (rust), eslint (js/ts), pyright (python), etc.

        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension {
            "rs" => self.run_rust_analyzer_diagnostics(file_path).await,
            "ts" | "js" | "tsx" | "jsx" => self.run_eslint_diagnostics(file_path).await,
            "py" => self.run_pyright_diagnostics(file_path).await,
            _ => Ok(DiagnosticSummary {
                total_count: 0,
                error_count: 0,
                warning_count: 0,
                info_count: 0,
                hint_count: 0,
                diagnostics: Vec::new(),
                has_errors: false,
            }),
        }
    }

    /// Run rust-analyzer diagnostics
    async fn run_rust_analyzer_diagnostics(
        &self,
        file_path: &PathBuf,
    ) -> Result<DiagnosticSummary, String> {
        // Try cargo check --message-format=json for Rust
        let output = Command::new("cargo")
            .args(["check", "--message-format=json"])
            .current_dir(&self.root)
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            // Parse JSON messages from cargo check
            let stderr = String::from_utf8_lossy(&output.stderr);
            let diagnostics = Self::parse_cargo_check_json(&stderr, file_path);

            return Ok(Self::summarize_diagnostics(&diagnostics));
        }

        Ok(DiagnosticSummary {
            total_count: 0,
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            hint_count: 0,
            diagnostics: Vec::new(),
            has_errors: false,
        })
    }

    /// Parse cargo check JSON output
    fn parse_cargo_check_json(output: &str, target_file: &PathBuf) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for line in output.lines() {
            if let Ok(message) = serde_json::from_str::<serde_json::Value>(line) {
                // Only process messages for our target file
                if let Some(spans) = message.get("spans")
                    && let Some(spans_array) = spans.as_array() {
                        for span in spans_array {
                            if let Some(file_path) = span.get("file_name").and_then(|v| v.as_str())
                            {
                                // Check if this span is for our target file
                                let span_file = PathBuf::from(file_path);
                                if span_file == *target_file {
                                    let message_text = message
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Unknown error")
                                        .to_string();

                                    let line = span
                                        .get("line_start")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(1)
                                        as usize;

                                    let column = span
                                        .get("character_start")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(1)
                                        as usize;

                                    let severity = message
                                        .get("level")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("error");

                                    let diag_severity = match severity {
                                        "error" => DiagnosticSeverity::Error,
                                        "warning" => DiagnosticSeverity::Warning,
                                        _ => DiagnosticSeverity::Information,
                                    };

                                    let code = message
                                        .get("code")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    diagnostics.push(Diagnostic {
                                        message: message_text,
                                        severity: diag_severity,
                                        file: span_file,
                                        line,
                                        column,
                                        code,
                                    });
                                }
                            }
                        }
                    }
            }
        }

        diagnostics
    }

    /// Run eslint diagnostics
    async fn run_eslint_diagnostics(
        &self,
        file_path: &PathBuf,
    ) -> Result<DiagnosticSummary, String> {
        let output = Command::new("npx")
            .args([
                "eslint",
                "--format",
                "json",
                file_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&self.root)
            .output()
            .map_err(|e| e.to_string())?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let diagnostics = Self::parse_eslint_json(&stdout, file_path);
            return Ok(Self::summarize_diagnostics(&diagnostics));
        }

        Ok(DiagnosticSummary {
            total_count: 0,
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            hint_count: 0,
            diagnostics: Vec::new(),
            has_errors: false,
        })
    }

    /// Parse eslint JSON output
    fn parse_eslint_json(output: &str, target_file: &PathBuf) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(output)
            && let Some(files) = json.as_array() {
                for file in files {
                    if let Some(file_path) = file.get("filePath").and_then(|v| v.as_str())
                        && PathBuf::from(file_path) == *target_file
                            && let Some(messages) = file.get("messages").and_then(|v| v.as_array())
                            {
                                for msg in messages {
                                    let message = msg
                                        .get("message")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Unknown")
                                        .to_string();

                                    let line = msg.get("line").and_then(|v| v.as_u64()).unwrap_or(1)
                                        as usize;

                                    let column =
                                        msg.get("column").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as usize;

                                    let severity =
                                        msg.get("severity").and_then(|v| v.as_u64()).unwrap_or(2);

                                    let diag_severity = match severity {
                                        1 => DiagnosticSeverity::Warning,
                                        2 => DiagnosticSeverity::Error,
                                        _ => DiagnosticSeverity::Information,
                                    };

                                    let rule_id = msg
                                        .get("ruleId")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());

                                    diagnostics.push(Diagnostic {
                                        message,
                                        severity: diag_severity,
                                        file: PathBuf::from(file_path),
                                        line,
                                        column,
                                        code: rule_id,
                                    });
                                }
                            }
                }
            }

        diagnostics
    }

    /// Run pyright diagnostics
    async fn run_pyright_diagnostics(
        &self,
        file_path: &PathBuf,
    ) -> Result<DiagnosticSummary, String> {
        let output = Command::new("npx")
            .args([
                "pyright",
                "--outputjson",
                file_path.to_string_lossy().as_ref(),
            ])
            .current_dir(&self.root)
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = Self::parse_pyright_json(&stdout, file_path);
        Ok(Self::summarize_diagnostics(&diagnostics))
    }

    /// Parse pyright JSON output
    fn parse_pyright_json(output: &str, target_file: &PathBuf) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(output)
            && let Some(diags) = json.get("generalDiagnostics").and_then(|v| v.as_array()) {
                for diag in diags {
                    if let Some(file_path) = diag.get("file").and_then(|v| v.as_str())
                        && PathBuf::from(file_path) == *target_file {
                            let message = diag
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            let line = diag
                                .get("range")
                                .and_then(|r| r.get("start"))
                                .and_then(|s| s.get("line"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(1) as usize;

                            let column = diag
                                .get("range")
                                .and_then(|r| r.get("start"))
                                .and_then(|s| s.get("character"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(1) as usize;

                            let severity = diag
                                .get("severity")
                                .and_then(|v| v.as_str())
                                .unwrap_or("error");

                            let diag_severity = match severity {
                                "error" => DiagnosticSeverity::Error,
                                "warning" => DiagnosticSeverity::Warning,
                                "information" => DiagnosticSeverity::Information,
                                "hint" => DiagnosticSeverity::Hint,
                                _ => DiagnosticSeverity::Information,
                            };

                            let code = diag
                                .get("rule")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            diagnostics.push(Diagnostic {
                                message,
                                severity: diag_severity,
                                file: PathBuf::from(file_path),
                                line,
                                column,
                                code,
                            });
                        }
                }
            }

        diagnostics
    }

    /// Summarize diagnostics
    fn summarize_diagnostics(diagnostics: &[Diagnostic]) -> DiagnosticSummary {
        let mut summary = DiagnosticSummary {
            total_count: diagnostics.len(),
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            hint_count: 0,
            diagnostics: diagnostics.to_vec(),
            has_errors: false,
        };

        for diag in diagnostics {
            match diag.severity {
                DiagnosticSeverity::Error => {
                    summary.error_count += 1;
                    summary.has_errors = true;
                }
                DiagnosticSeverity::Warning => summary.warning_count += 1,
                DiagnosticSeverity::Information => summary.info_count += 1,
                DiagnosticSeverity::Hint => summary.hint_count += 1,
            }
        }

        summary
    }

    /// Format diagnostics for display
    pub fn format_diagnostics(summary: &DiagnosticSummary) -> String {
        if summary.diagnostics.is_empty() {
            return String::from("No diagnostics found.");
        }

        let mut output = String::new();

        output.push_str(&format!("Found {} diagnostic(s):\n\n", summary.total_count));

        for diag in &summary.diagnostics {
            let severity_marker = match diag.severity {
                DiagnosticSeverity::Error => "ERROR",
                DiagnosticSeverity::Warning => "WARNING",
                DiagnosticSeverity::Information => "INFO",
                DiagnosticSeverity::Hint => "HINT",
            };

            let code_str = if let Some(ref code) = diag.code {
                format!(" [{}]", code)
            } else {
                String::new()
            };

            output.push_str(&format!(
                "{}: {}:{}:{} {}{}\n",
                severity_marker,
                diag.file.display(),
                diag.line,
                diag.column,
                diag.message,
                code_str
            ));
        }

        output
    }
}

/// LSP Diagnostics manager
#[derive(Debug, Clone)]
pub struct LspDiagnostics {
    clients: HashMap<String, LspClient>,
    enabled: bool,
}

impl LspDiagnostics {
    /// Create a new diagnostics manager
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            enabled: true,
        }
    }

    /// Add an LSP client for a language
    pub fn add_client(&mut self, language: &str, client: LspClient) {
        self.clients.insert(language.to_string(), client);
    }

    /// Get diagnostics for a file
    pub async fn get_diagnostics(&self, file_path: &PathBuf) -> Result<DiagnosticSummary, String> {
        if !self.enabled {
            return Ok(DiagnosticSummary {
                total_count: 0,
                error_count: 0,
                warning_count: 0,
                info_count: 0,
                hint_count: 0,
                diagnostics: Vec::new(),
                has_errors: false,
            });
        }

        // Determine language from extension
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let language = match extension {
            "rs" => "rust",
            "ts" | "tsx" | "js" | "jsx" => "typescript",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            _ => "generic",
        };

        // Try to use configured client or run diagnostics directly
        if let Some(client) = self.clients.get(language) {
            client.get_diagnostics(file_path).await
        } else {
            // Use generic client
            let generic_client = LspClient::new(vec![], file_path.clone());
            generic_client.get_diagnostics(file_path).await
        }
    }

    /// Enable or disable diagnostics
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl Default for LspDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_summary() {
        let diagnostics = vec![
            Diagnostic {
                message: "Error 1".to_string(),
                severity: DiagnosticSeverity::Error,
                file: PathBuf::from("test.rs"),
                line: 1,
                column: 1,
                code: Some("E0001".to_string()),
            },
            Diagnostic {
                message: "Warning 1".to_string(),
                severity: DiagnosticSeverity::Warning,
                file: PathBuf::from("test.rs"),
                line: 2,
                column: 5,
                code: None,
            },
        ];

        let summary = LspClient::summarize_diagnostics(&diagnostics);

        assert_eq!(summary.total_count, 2);
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.warning_count, 1);
        assert!(summary.has_errors);
    }

    #[test]
    fn test_format_diagnostics() {
        let diagnostics = vec![Diagnostic {
            message: "Unused variable".to_string(),
            severity: DiagnosticSeverity::Warning,
            file: PathBuf::from("src/main.rs"),
            line: 10,
            column: 5,
            code: Some("unused_variables".to_string()),
        }];

        let summary = LspClient::summarize_diagnostics(&diagnostics);
        let formatted = LspClient::format_diagnostics(&summary);

        assert!(formatted.contains("WARNING"));
        assert!(formatted.contains("src/main.rs:10:5"));
        assert!(formatted.contains("unused_variables"));
    }
}
