//! Output Truncation - Large output handling
//!
//! Responsibilities:
//! - Detect oversized output
//! - Truncate output with hints
//! - Save full output to disk
//! - Provide offset/limit for partial reads

use std::fs::File;
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::warn;

/// Maximum number of lines before truncation
pub const MAX_LINES: usize = 2000;
/// Maximum bytes before truncation
pub const MAX_BYTES: usize = 50 * 1024; // 50KB
/// Default output directory
const DEFAULT_OUTPUT_DIR: &str = "/tmp/ndc-outputs";

/// Truncated output result
#[derive(Debug, Clone)]
pub struct TruncatedOutput {
    /// The output content (possibly truncated)
    pub content: String,
    /// Whether the output was truncated
    pub truncated: bool,
    /// Path to full output file if saved
    pub output_path: Option<PathBuf>,
    /// Original size in bytes
    pub original_size: usize,
    /// Number of lines in original output
    pub line_count: usize,
}

/// Output truncation configuration
#[derive(Debug, Clone)]
pub struct TruncationConfig {
    /// Maximum lines before truncation
    pub max_lines: usize,
    /// Maximum bytes before truncation
    pub max_bytes: usize,
    /// Directory for saved outputs
    pub output_dir: PathBuf,
    /// Whether to save truncated output to disk
    pub save_to_disk: bool,
    /// Head lines to keep when truncated
    pub head_lines: usize,
    /// Tail lines to keep when truncated
    pub tail_lines: usize,
}

impl Default for TruncationConfig {
    fn default() -> Self {
        Self {
            max_lines: MAX_LINES,
            max_bytes: MAX_BYTES,
            output_dir: PathBuf::from(DEFAULT_OUTPUT_DIR),
            save_to_disk: true,
            head_lines: 100,
            tail_lines: 100,
        }
    }
}

/// Output truncation handler
pub struct OutputTruncator {
    config: TruncationConfig,
    temp_dir: Option<TempDir>,
}

impl OutputTruncator {
    /// Create a new truncator with default config
    pub fn new() -> Self {
        Self::with_config(TruncationConfig::default())
    }

    /// Create a truncator with custom config
    pub fn with_config(config: TruncationConfig) -> Self {
        Self {
            config,
            temp_dir: None,
        }
    }

    /// Truncate output if necessary
    pub fn truncate(&mut self, output: &str) -> TruncatedOutput {
        let line_count = output.lines().count();
        let byte_count = output.len();

        // Check if truncation is needed
        let needs_truncation = line_count > self.config.max_lines || byte_count > self.config.max_bytes;

        if !needs_truncation {
            return TruncatedOutput {
                content: output.to_string(),
                truncated: false,
                output_path: None,
                original_size: byte_count,
                line_count,
            };
        }

        // Truncate the output
        self.truncate_output(output, line_count, byte_count)
    }

    /// Truncate output and optionally save to disk
    fn truncate_output(&mut self, output: &str, line_count: usize, byte_count: usize) -> TruncatedOutput {
        let lines: Vec<&str> = output.lines().collect();
        let total_lines = lines.len();

        // Keep head and tail
        let head_end = std::cmp::min(self.config.head_lines, total_lines);
        let tail_start = std::cmp::max(total_lines.saturating_sub(self.config.tail_lines), head_end);

        let head: Vec<&str> = lines[..head_end].to_vec();
        let tail: Vec<&str> = lines[tail_start..].to_vec();

        // Format truncated content
        let truncated_content = format!(
            "{}\n\n... truncated ({}/{} lines, {} bytes) ...\n\n{}\n\nHint: Use read tool with offset/limit to view specific portions. Full output saved to: {}",
            head.join("\n"),
            total_lines - head_end - tail.len(),
            total_lines,
            byte_count,
            tail.join("\n"),
            if self.config.save_to_disk {
                "see output_path below"
            } else {
                "(disk save disabled)"
            }
        );

        // Save to disk if enabled
        let output_path = if self.config.save_to_disk {
            Some(self.save_to_disk(output, byte_count))
        } else {
            None
        };

        TruncatedOutput {
            content: truncated_content,
            truncated: true,
            output_path,
            original_size: byte_count,
            line_count,
        }
    }

    /// Save output to disk
    fn save_to_disk(&mut self, output: &str, _byte_count: usize) -> PathBuf {
        // Ensure output directory exists
        let output_dir = &self.config.output_dir;

        // Use temp dir if configured output dir doesn't exist
        let dir_path = if !output_dir.exists() {
            let temp = TempDir::new().expect("Failed to create temp dir");
            self.temp_dir = Some(temp);
            self.temp_dir.as_ref().unwrap().path().to_path_buf()
        } else {
            output_dir.clone()
        };

        // Generate unique filename
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f");
        let filename = format!("ndc_output_{}.txt", timestamp);
        let file_path = dir_path.join(&filename);

        // Write to file
        if let Err(e) = std::fs::write(&file_path, output) {
            warn!("Failed to save output to disk: {}", e);
            // Try temp dir as fallback
            let fallback_path = std::env::temp_dir().join(&filename);
            if let Err(e2) = std::fs::write(&fallback_path, output) {
                warn!("Failed to save to temp dir: {}", e2);
            }
            fallback_path
        } else {
            file_path
        }
    }

    /// Get config
    pub fn config(&self) -> &TruncationConfig {
        &self.config
    }

    /// Update config
    pub fn set_config(&mut self, config: TruncationConfig) {
        self.config = config;
    }
}

impl Default for OutputTruncator {
    fn default() -> Self {
        Self::new()
    }
}

/// Read partial content from a saved output file
pub fn read_partial_output(file_path: &PathBuf, offset: usize, limit: Option<usize>) -> Result<String, String> {
    if !file_path.exists() {
        return Err(format!("Output file not found: {}", file_path.display()));
    }

    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read output file: {}", e))?;

    let lines: Vec<&str> = content.lines().collect();
    let start = std::cmp::min(offset, lines.len());
    let end = match limit {
        Some(limit) => std::cmp::min(start + limit, lines.len()),
        None => lines.len(),
    };

    if start >= end {
        return Ok(String::new());
    }

    Ok(lines[start..end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_output_not_truncated() {
        let mut truncator = OutputTruncator::new();
        let output = "Hello\nWorld";

        let result = truncator.truncate(output);

        assert!(!result.truncated);
        assert_eq!(result.content, output);
        assert!(result.output_path.is_none());
        assert_eq!(result.original_size, 11);
        assert_eq!(result.line_count, 2);
    }

    #[test]
    fn test_large_output_truncated() {
        let mut truncator = OutputTruncator::new();
        let large_output: String = (1..=3000).map(|i| format!("Line {}\n", i)).collect();

        let result = truncator.truncate(&large_output);

        assert!(result.truncated);
        assert!(result.content.contains("... truncated"));
        assert!(result.output_path.is_some());
        assert_eq!(result.original_size, large_output.len());
        assert_eq!(result.line_count, 3000);
    }

    #[test]
    fn test_output_saved_to_disk() {
        let mut truncator = OutputTruncator::new();
        let large_output: String = (1..=3000).map(|i| format!("Line {}\n", i)).collect();

        let result = truncator.truncate(&large_output);

        if let Some(path) = &result.output_path {
            assert!(path.exists());
            let saved_content = std::fs::read_to_string(path).unwrap();
            assert_eq!(saved_content, large_output);
        }
    }

    #[test]
    fn test_partial_read() {
        let mut truncator = OutputTruncator::new();
        let output: String = (1..=100).map(|i| format!("Line {}\n", i)).collect();

        let result = truncator.truncate(&output);

        if let Some(path) = &result.output_path {
            let partial = read_partial_output(path, 0, Some(10)).unwrap();
            assert!(partial.contains("Line 1"));
            assert!(partial.contains("Line 10"));
        }
    }

    #[test]
    fn test_custom_config() {
        let config = TruncationConfig {
            max_lines: 10,
            max_bytes: 100,
            output_dir: PathBuf::from("/tmp"),
            save_to_disk: false,
            head_lines: 2,
            tail_lines: 2,
        };

        let mut truncator = OutputTruncator::with_config(config);
        let output: String = (1..=50).map(|i| format!("Line {}\n", i)).collect();

        let result = truncator.truncate(&output);

        assert!(result.truncated);
        assert!(!result.output_path.is_some()); // save_to_disk is false
    }
}
