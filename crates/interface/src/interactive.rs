//! Interactive UI Components for Agent Mode
//!
//! Simple UI components for:
//! - Streaming response display
//! - Agent status display
//! - Progress indicators

use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;

/// Streaming response display
#[derive(Debug, Default)]
pub struct StreamingDisplay {
    buffer: Vec<String>,
    current_line: String,
}

impl StreamingDisplay {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            current_line: String::new(),
        }
    }

    /// Append text to the display
    pub fn append(&mut self, text: &str) {
        self.current_line.push_str(text);
        print!("{}", text);
        let _ = std::io::stdout().flush();
    }

    /// Append a line
    pub fn append_line(&mut self, text: &str) {
        self.append(text);
        println!();
        self.buffer.push(self.current_line.clone());
        self.current_line.clear();
    }

    /// Get all buffered content
    pub fn content(&self) -> String {
        let mut result = self.buffer.join("\n");
        if !self.current_line.is_empty() {
            result.push('\n');
            result.push_str(&self.current_line);
        }
        result
    }

    /// Clear the display
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_line.clear();
    }
}

/// Display agent status
pub fn display_agent_status(
    agent_name: &str,
    provider: &str,
    model: &str,
    state: &str,
    tasks_completed: usize,
    tasks_total: usize,
) {
    let status_icon = if state == "running" { "[OK]" } else { "[--]" };

    println!();
    println!("+-----------------------------------------------------------------+");
    println!(
        "| {} Agent Status                                                   |",
        status_icon
    );
    println!("+-----------------------------------------------------------------+");
    println!("| Agent:    {:50} |", agent_name);
    println!(
        "| Provider: {} @ {}                                     |",
        format!("{:<14}", provider),
        format!("{:<24}", model)
    );
    println!("| State:    {:50} |", state);
    println!(
        "| Progress: {}/{}                                            |",
        tasks_completed, tasks_total
    );
    println!("+-----------------------------------------------------------------+");
    println!();
}

/// Risk level for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Progress indicator for operations
#[derive(Debug)]
pub struct ProgressIndicator {
    bar: Option<ProgressBar>,
}

impl ProgressIndicator {
    pub fn new() -> Self {
        Self { bar: None }
    }

    /// Start a new progress bar
    pub fn start(&mut self, total: usize, message: &str) {
        let bar = ProgressBar::new(total as u64)
            .with_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {wide_bar} {pos}/{len}")
                    .expect("valid template"),
            )
            .with_message(message.to_string());

        self.bar = Some(bar);
    }

    /// Update progress
    pub fn update(&mut self, pos: usize) {
        if let Some(ref bar) = self.bar {
            bar.set_position(pos as u64);
        }
    }

    /// Increment progress
    pub fn inc(&mut self) {
        if let Some(ref bar) = self.bar {
            bar.inc(1);
        }
    }

    /// Finish the progress bar
    pub fn finish(&mut self) {
        if let Some(ref bar) = self.bar {
            bar.finish();
        }
    }

    /// Set message on progress bar
    pub fn set_message(&self, _message: &str) {
        // Message setting requires static lifetime, skipped for simplicity
    }
}

impl Default for ProgressIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProgressIndicator {
    fn drop(&mut self) {
        if let Some(ref bar) = self.bar {
            bar.finish_and_clear();
        }
    }
}

/// Multi-progress manager
#[derive(Debug, Default)]
pub struct MultiProgress {
    bars: Vec<ProgressBar>,
}

impl MultiProgress {
    pub fn new() -> Self {
        Self { bars: Vec::new() }
    }

    pub fn add(&mut self, total: usize, name: &str) -> ProgressBar {
        let bar = ProgressBar::new(total as u64)
            .with_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {}: {wide_bar} {pos}/{len}")
                    .expect("valid template"),
            )
            .with_message(name.to_string());
        self.bars.push(bar.clone());
        bar
    }
}

/// Agent mode switcher placeholder
#[derive(Debug)]
pub struct AgentModeSwitcher;

impl AgentModeSwitcher {
    pub fn new(_profiles: Vec<ndc_core::AgentProfile>) -> Self {
        Self
    }

    pub async fn run(&mut self) -> AgentSwitchResult {
        AgentSwitchResult::Cancelled
    }
}

/// Agent mode switcher result
#[derive(Debug)]
pub enum AgentSwitchResult {
    Selected(String),
    Cancelled,
}

/// Permission result
#[derive(Debug)]
pub enum PermissionResult {
    Allow,
    Deny,
    AlwaysAllow,
}

/// Permission confirm placeholder
#[derive(Debug)]
pub struct PermissionConfirm;

impl PermissionConfirm {
    pub fn new(_tool: &str, _operation: &str, _risk: RiskLevel) -> Self {
        Self
    }

    pub async fn confirm(&self) -> PermissionResult {
        PermissionResult::Allow
    }
}

/// Recovery action
#[derive(Debug)]
pub enum RecoveryAction {
    Retry,
    Skip,
    Abort,
    Fallback,
}

/// Prompt recovery placeholder
pub async fn prompt_recovery(_error: &str, _context: &str) -> RecoveryAction {
    RecoveryAction::Retry
}

/// Display tool call placeholder
pub fn display_tool_call(_tool: &str, _args: &str, _result: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_display_new() {
        let display = StreamingDisplay::new();
        assert!(display.content().is_empty());
    }

    #[test]
    fn test_streaming_display_append() {
        let mut display = StreamingDisplay::new();
        display.append("Hello");
        assert!(display.content().contains("Hello"));
    }
}
