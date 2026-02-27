//! Prompt & message construction — builds the LLM message list from session
//! history, system prompt, and working memory context.
//!
//! Extracted from `orchestrator.rs` to reduce god-object complexity.

use super::{
    AgentError, AgentSession,
    injectors::working_memory::{TaskContext, WorkingMemoryContext, WorkingMemoryInjector},
    prompts::{EnhancedPromptContext, build_enhanced_prompt},
};
use crate::TaskId;
use crate::llm::provider::{Message, MessageRole, ToolCall, ToolCallFunction};
use tracing::warn;

/// Build the complete LLM message list from session history + user input.
///
/// Handles:
/// - System prompt construction (template or enhanced prompt)
/// - History reconstruction with legacy tool_call_id recovery
/// - Orphaned tool_use / tool_result cleanup
pub(crate) fn build_messages(
    session: &AgentSession,
    user_message: &Message,
    active_task_id: Option<TaskId>,
    working_dir: Option<std::path::PathBuf>,
    working_memory: Option<crate::WorkingMemory>,
    system_prompt_template: &Option<String>,
    tool_schemas: Vec<serde_json::Value>,
) -> Result<Vec<Message>, AgentError> {
    let mut messages = Vec::new();

    // 构建系统提示词
    let working_memory_injector =
        build_working_memory_injector(session, active_task_id, working_dir.clone(), working_memory);
    let prompt_context = EnhancedPromptContext {
        available_tools: tool_schemas,
        active_task_id,
        working_dir,
        working_memory: Some(working_memory_injector),
        invariants: None,
        lineage: None,
        context_patterns: Vec::new(),
    };

    let system_prompt = if let Some(template) = system_prompt_template {
        template.clone()
    } else {
        build_enhanced_prompt(&prompt_context)
    };

    messages.push(Message {
        role: MessageRole::System,
        content: system_prompt,
        name: None,
        tool_calls: None,
    });

    // 添加历史消息 (最近的 N 条)
    // 为兼容旧 session 数据（Tool 消息缺少 tool_call_id），
    // 从前一条 Assistant 消息的 tool_calls 中依次恢复。
    let history_limit = 20;
    let mut pending_tc_ids: Vec<String> = Vec::new();
    for msg in session.messages.iter().rev().take(history_limit).rev() {
        let tool_calls = msg.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| ToolCall {
                    id: tc.id.clone(),
                    function: ToolCallFunction {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                })
                .collect()
        });

        // Assistant 消息带 tool_calls 时，缓存其 ID 列表
        if msg.role == MessageRole::Assistant {
            if let Some(ref tcs) = msg.tool_calls {
                pending_tc_ids = tcs.iter().map(|tc| tc.id.clone()).collect();
            } else {
                pending_tc_ids.clear();
            }
        }

        // Tool 消息：优先使用已有 tool_call_id，否则从缓存的 ID 队列中取
        let name = if msg.role == MessageRole::Tool {
            msg.tool_call_id.clone().or_else(|| {
                if !pending_tc_ids.is_empty() {
                    Some(pending_tc_ids.remove(0))
                } else {
                    None
                }
            })
        } else {
            msg.tool_call_id.clone()
        };

        // 如果 Tool 消息仍然缺少 tool_call_id，跳过以免 API 报错
        if msg.role == MessageRole::Tool && name.is_none() {
            warn!("Skipping Tool message without tool_call_id in session history");
            continue;
        }

        messages.push(Message {
            role: msg.role.clone(),
            content: msg.content.clone(),
            name,
            tool_calls,
        });
    }

    // 后处理：验证 tool_use / tool_result 配对完整性。
    // 收集所有 tool_use_id 和 tool_result_id，移除不匹配的条目。
    {
        use std::collections::HashSet;
        let tool_use_ids: HashSet<String> = messages
            .iter()
            .filter_map(|m| m.tool_calls.as_ref())
            .flat_map(|tcs| tcs.iter().map(|tc| tc.id.clone()))
            .collect();
        let tool_result_ids: HashSet<String> = messages
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .filter_map(|m| m.name.clone())
            .collect();

        // 如果存在不匹配的 ID，清理掉孤立的 tool_use 和 tool_result
        let orphan_uses: HashSet<&String> = tool_use_ids.difference(&tool_result_ids).collect();
        let orphan_results: HashSet<&String> = tool_result_ids.difference(&tool_use_ids).collect();

        if !orphan_uses.is_empty() || !orphan_results.is_empty() {
            if !orphan_uses.is_empty() {
                warn!(
                    "Stripping {} orphaned tool_use(s) from history",
                    orphan_uses.len()
                );
            }
            if !orphan_results.is_empty() {
                warn!(
                    "Removing {} orphaned tool_result(s) from history",
                    orphan_results.len()
                );
            }
            // 移除孤立的 tool_result 消息
            messages.retain(|m| {
                if m.role == MessageRole::Tool
                    && let Some(ref id) = m.name
                {
                    return !orphan_results.contains(id);
                }
                true
            });
            // 对于孤立的 tool_use：从 Assistant 消息中去掉 tool_calls
            for m in messages.iter_mut() {
                if let Some(ref mut tcs) = m.tool_calls {
                    tcs.retain(|tc| !orphan_uses.contains(&tc.id));
                    if tcs.is_empty() {
                        m.tool_calls = None;
                    }
                }
            }
        }
    }

    // 添加当前用户消息
    messages.push(user_message.clone());

    Ok(messages)
}

/// Build a WorkingMemoryInjector from session context and optional explicit working memory.
pub(crate) fn build_working_memory_injector(
    session: &AgentSession,
    active_task_id: Option<TaskId>,
    _working_dir: Option<std::path::PathBuf>,
    working_memory: Option<crate::WorkingMemory>,
) -> WorkingMemoryInjector {
    let mut injector = WorkingMemoryInjector::default();

    if let Some(ref wm) = working_memory {
        injector.update(WorkingMemoryInjector::from_working_memory(wm));
        return injector;
    }

    let recent_failures: Vec<String> = session
        .messages
        .iter()
        .rev()
        .filter_map(|m| {
            let lower = m.content.to_lowercase();
            if lower.contains("error") || lower.contains("failed") || lower.contains("panic") {
                Some(m.content.clone())
            } else {
                None
            }
        })
        .take(5)
        .collect();

    let mut context = WorkingMemoryContext {
        abstract_summary: None,
        raw_summary: None,
        hard_constraints: Vec::new(),
        active_files: Vec::new(),
        api_surface: Vec::new(),
        recent_failures,
        current_task: None,
        custom: std::collections::HashMap::new(),
    };

    if let Some(task_id) = active_task_id {
        context.current_task = Some(TaskContext {
            task_id: task_id.to_string(),
            task_title: "active task".to_string(),
            current_step: "continue execution".to_string(),
            completed_steps: Vec::new(),
        });
        context.raw_summary = Some("Task-scoped execution context active".to_string());
    }

    if !context.recent_failures.is_empty() {
        context.abstract_summary = Some("Recent failures detected in this session".to_string());
    }

    injector.update(context);
    injector
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_agent::{AgentMessage, AgentSession, AgentToolCall};

    #[test]
    fn test_build_messages_includes_system_prompt_and_user() {
        let session = AgentSession::new("test-session".to_string());
        let user_msg = Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            name: None,
            tool_calls: None,
        };

        let messages =
            build_messages(&session, &user_msg, None, None, None, &None, vec![]).unwrap();

        // Should have system prompt + user message
        assert!(messages.len() >= 2);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages.last().unwrap().role, MessageRole::User);
        assert_eq!(messages.last().unwrap().content, "hello");
    }

    #[test]
    fn test_build_messages_uses_custom_template() {
        let session = AgentSession::new("test-session".to_string());
        let user_msg = Message {
            role: MessageRole::User,
            content: "hi".to_string(),
            name: None,
            tool_calls: None,
        };

        let template = Some("Custom system prompt".to_string());
        let messages =
            build_messages(&session, &user_msg, None, None, None, &template, vec![]).unwrap();

        assert_eq!(messages[0].content, "Custom system prompt");
    }

    #[test]
    fn test_build_messages_recovers_legacy_tool_call_id() {
        let mut session = AgentSession::new("legacy-session".to_string());
        session.project_id = "test-project".to_string();

        session.add_message(AgentMessage {
            role: MessageRole::User,
            content: "run tool".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
            tool_call_id: None,
        });

        session.add_message(AgentMessage {
            role: MessageRole::Assistant,
            content: "".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: Some(vec![AgentToolCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"src/main.rs"}"#.to_string(),
                id: "call_abc123".to_string(),
            }]),
            tool_results: None,
            tool_call_id: None,
        });

        // Tool result WITHOUT tool_call_id (legacy data)
        session.add_message(AgentMessage {
            role: MessageRole::Tool,
            content: "fn main() {}".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: Some(vec!["fn main() {}".to_string()]),
            tool_call_id: None,
        });

        let user_msg = Message {
            role: MessageRole::User,
            content: "next".to_string(),
            name: None,
            tool_calls: None,
        };

        let messages =
            build_messages(&session, &user_msg, None, None, None, &None, vec![]).unwrap();

        let tool_msg = messages
            .iter()
            .find(|m| m.role == MessageRole::Tool)
            .expect("should have a Tool message");
        assert_eq!(tool_msg.name.as_deref(), Some("call_abc123"));
    }

    #[test]
    fn test_build_messages_skips_orphaned_tool_messages() {
        let mut session = AgentSession::new("orphan-session".to_string());
        session.project_id = "test-project".to_string();

        // Orphaned Tool message — no preceding Assistant with tool_calls
        session.add_message(AgentMessage {
            role: MessageRole::Tool,
            content: "some result".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: Some(vec!["some result".to_string()]),
            tool_call_id: None,
        });

        session.add_message(AgentMessage {
            role: MessageRole::User,
            content: "continue".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
            tool_call_id: None,
        });

        let user_msg = Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            name: None,
            tool_calls: None,
        };

        let messages =
            build_messages(&session, &user_msg, None, None, None, &None, vec![]).unwrap();

        assert!(
            !messages.iter().any(|m| m.role == MessageRole::Tool),
            "Orphaned Tool message should be skipped"
        );
    }

    #[test]
    fn test_working_memory_injector_from_explicit_memory() {
        let session = AgentSession::new("test-session".to_string());
        let wm = crate::WorkingMemory {
            scope: crate::SubTaskId("task-1".to_string()),
            abstract_history: crate::AbstractHistory {
                failure_patterns: vec![],
                root_cause_summary: Some("test root cause".to_string()),
                attempt_count: 2,
                trajectory_state: crate::TrajectoryState::Progressing {
                    steps_since_last_failure: 1,
                },
            },
            raw_current: crate::RawCurrent {
                active_files: vec![std::path::PathBuf::from("src/main.rs")],
                api_surface: vec![],
                current_step_context: None,
            },
            hard_invariants: vec![],
        };

        let injector = build_working_memory_injector(&session, None, None, Some(wm));
        let rendered = injector.inject();
        assert!(!rendered.is_empty());
    }

    #[test]
    fn test_working_memory_injector_extracts_failures() {
        let mut session = AgentSession::new("test-session".to_string());
        session.add_message(AgentMessage {
            role: MessageRole::Assistant,
            content: "Error: compilation failed".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
            tool_call_id: None,
        });
        session.add_message(AgentMessage {
            role: MessageRole::Assistant,
            content: "All good here".to_string(),
            timestamp: chrono::Utc::now(),
            tool_calls: None,
            tool_results: None,
            tool_call_id: None,
        });

        let injector = build_working_memory_injector(&session, None, None, None);
        let rendered = injector.inject();
        // Should contain failure-related content
        assert!(
            rendered.contains("failure")
                || rendered.contains("error")
                || rendered.contains("Recent"),
            "Injector should surface recent failures"
        );
    }
}
