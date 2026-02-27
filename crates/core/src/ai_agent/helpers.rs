//! Utility helpers for the agent orchestration layer.
//!
//! Pure functions for text truncation, sanitization, and message management.
//! Extracted from `orchestrator.rs` to reduce god-object complexity.

use crate::llm::provider::{Message, MessageRole, ToolCall};

/// Maximum characters for tool output before truncation.
pub(crate) const MAX_TOOL_OUTPUT_CHARS: usize = 100_000;

/// Maximum number of messages to keep in the conversation history.
/// System prompt is always preserved. When exceeded, older messages (after system
/// prompt) are replaced with a summary placeholder.
pub(crate) const MAX_CONVERSATION_MESSAGES: usize = 40;

/// Truncate content for inclusion in execution events.
pub(crate) fn truncate_for_event(content: &str, max: usize) -> String {
    let trimmed = content.trim();
    if trimmed.len() <= max {
        return trimmed.to_string();
    }
    let mut out = trimmed.chars().take(max).collect::<String>();
    out.push_str("...");
    out
}

/// Sanitize tool output: truncate if too long, wrap with XML boundary tags.
pub(crate) fn sanitize_tool_output(content: &str) -> String {
    let truncated = if content.len() > MAX_TOOL_OUTPUT_CHARS {
        let mut out = content
            .chars()
            .take(MAX_TOOL_OUTPUT_CHARS)
            .collect::<String>();
        out.push_str("\n[truncated — output exceeded limit]");
        out
    } else {
        content.to_string()
    };
    format!("<tool_output>\n{}\n</tool_output>", truncated)
}

/// Check whether the message is a confirmation-permission error sentinel.
pub(crate) fn is_confirmation_permission_error(message: &str) -> bool {
    message
        .trim_start()
        .starts_with("requires_confirmation permission=")
}

/// Truncate messages to bounded size, preserving system prompt and recent history.
///
/// - Keeps the first message if it's a system prompt
/// - Keeps the most recent `max_messages` non-system messages
/// - Replaces removed messages with a single placeholder
pub(crate) fn truncate_messages(messages: &mut Vec<Message>, max_messages: usize) {
    // Find system prompt boundary
    let system_count = if messages
        .first()
        .is_some_and(|m| m.role == MessageRole::System)
    {
        1
    } else {
        0
    };

    let non_system = messages.len() - system_count;
    if non_system <= max_messages {
        return;
    }

    let to_remove = non_system - max_messages;
    let placeholder = Message {
        role: MessageRole::User,
        content: "[earlier conversation history omitted for context window management]".to_string(),
        name: None,
        tool_calls: None,
    };

    // Remove messages after system prompt, replace with single placeholder
    messages.splice(system_count..system_count + to_remove, [placeholder]);
}

/// Compact multi-line content into a single-line preview.
pub(crate) fn compact_preview(content: &str, max: usize) -> String {
    let one_line = content
        .replace(['\n', '\r'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_for_event(&one_line, max)
}

/// Build a human-readable summary of planned tool calls.
pub(crate) fn summarize_tool_calls(tool_calls: &[ToolCall]) -> String {
    let mut parts = Vec::new();
    for call in tool_calls.iter().take(3) {
        let arg = compact_preview(&call.function.arguments, 60);
        parts.push(format!("{}({})", call.function.name, arg));
    }
    let mut summary = format!("planning tool calls: {}", parts.join(", "));
    if tool_calls.len() > 3 {
        summary.push_str(&format!(", ... +{} more", tool_calls.len() - 3));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::provider::ToolCallFunction;

    // ---- sanitize_tool_output ----

    #[test]
    fn test_sanitize_tool_output_short() {
        let content = "Hello world";
        let sanitized = sanitize_tool_output(content);
        assert!(sanitized.starts_with("<tool_output>"));
        assert!(sanitized.ends_with("</tool_output>"));
        assert!(sanitized.contains("Hello world"));
        assert!(!sanitized.contains("[truncated"));
    }

    #[test]
    fn test_sanitize_tool_output_truncated() {
        let content = "x".repeat(MAX_TOOL_OUTPUT_CHARS + 1000);
        let sanitized = sanitize_tool_output(&content);
        assert!(sanitized.starts_with("<tool_output>"));
        assert!(sanitized.ends_with("</tool_output>"));
        assert!(sanitized.contains("[truncated"));
        // The inner content should be at most MAX_TOOL_OUTPUT_CHARS + truncation marker
        let inner = sanitized
            .strip_prefix("<tool_output>\n")
            .unwrap()
            .strip_suffix("\n</tool_output>")
            .unwrap();
        assert!(inner.len() < content.len());
    }

    #[test]
    fn test_sanitize_tool_output_exactly_at_limit() {
        let content = "a".repeat(MAX_TOOL_OUTPUT_CHARS);
        let sanitized = sanitize_tool_output(&content);
        assert!(!sanitized.contains("[truncated"));
        assert!(sanitized.contains(&content));
    }

    // ---- truncate_messages ----

    fn make_msg(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            name: None,
            tool_calls: None,
        }
    }

    #[test]
    fn test_truncate_messages_under_limit() {
        let mut msgs = vec![
            make_msg(MessageRole::System, "system"),
            make_msg(MessageRole::User, "hello"),
            make_msg(MessageRole::Assistant, "hi"),
        ];
        truncate_messages(&mut msgs, 40);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "system");
    }

    #[test]
    fn test_truncate_messages_over_limit() {
        let mut msgs = vec![make_msg(MessageRole::System, "system")];
        for i in 0..10 {
            msgs.push(make_msg(MessageRole::User, &format!("u{}", i)));
            msgs.push(make_msg(MessageRole::Assistant, &format!("a{}", i)));
        }
        // 1 system + 20 non-system = 21 total. Limit to 6 non-system.
        truncate_messages(&mut msgs, 6);
        // system + placeholder + 6 recent = 8
        assert_eq!(msgs.len(), 8);
        assert_eq!(msgs[0].role, MessageRole::System);
        assert!(msgs[1].content.contains("earlier conversation"));
        // Last message should be a9 (the most recent assistant)
        assert_eq!(msgs[7].content, "a9");
    }

    #[test]
    fn test_truncate_messages_no_system_prompt() {
        let mut msgs = vec![];
        for i in 0..10 {
            msgs.push(make_msg(MessageRole::User, &format!("u{}", i)));
        }
        truncate_messages(&mut msgs, 4);
        // placeholder + 4 recent = 5
        assert_eq!(msgs.len(), 5);
        assert!(msgs[0].content.contains("earlier conversation"));
        assert_eq!(msgs[4].content, "u9");
    }

    #[test]
    fn test_truncate_messages_exactly_at_limit() {
        let mut msgs = vec![make_msg(MessageRole::System, "system")];
        for i in 0..5 {
            msgs.push(make_msg(MessageRole::User, &format!("u{}", i)));
        }
        // 1 system + 5 non-system. Limit = 5 → no truncation
        truncate_messages(&mut msgs, 5);
        assert_eq!(msgs.len(), 6);
    }

    // ---- truncate_for_event ----

    #[test]
    fn test_truncate_for_event_short() {
        assert_eq!(truncate_for_event("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_for_event_long() {
        let result = truncate_for_event("abcdefghij", 5);
        assert_eq!(result, "abcde...");
    }

    #[test]
    fn test_truncate_for_event_trims_whitespace() {
        assert_eq!(truncate_for_event("  hello  ", 100), "hello");
    }

    // ---- compact_preview ----

    #[test]
    fn test_compact_preview_multiline() {
        let input = "line1\nline2\nline3";
        let result = compact_preview(input, 100);
        assert!(!result.contains('\n'));
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
    }

    // ---- is_confirmation_permission_error ----

    #[test]
    fn test_is_confirmation_permission_error_true() {
        assert!(is_confirmation_permission_error(
            "requires_confirmation permission=write_file"
        ));
    }

    #[test]
    fn test_is_confirmation_permission_error_false() {
        assert!(!is_confirmation_permission_error("some other error"));
    }

    // ---- summarize_tool_calls ----

    #[test]
    fn test_summarize_tool_calls_single() {
        let calls = vec![ToolCall {
            id: "call_1".to_string(),
            function: ToolCallFunction {
                name: "read_file".to_string(),
                arguments: r#"{"path":"src/main.rs"}"#.to_string(),
            },
        }];
        let summary = summarize_tool_calls(&calls);
        assert!(summary.contains("read_file"));
        assert!(summary.contains("src/main.rs"));
    }

    #[test]
    fn test_summarize_tool_calls_many() {
        let calls: Vec<ToolCall> = (0..5)
            .map(|i| ToolCall {
                id: format!("call_{}", i),
                function: ToolCallFunction {
                    name: format!("tool_{}", i),
                    arguments: "{}".to_string(),
                },
            })
            .collect();
        let summary = summarize_tool_calls(&calls);
        assert!(summary.contains("+2 more"));
    }
}
