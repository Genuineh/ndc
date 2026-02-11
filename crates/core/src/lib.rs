// NDC Core - 核心数据模型
//!
//! 包含：
//! - Task: 任务模型（Task-Intent 统一）
//! - Intent/Verdict: 决策引擎类型
//! - Agent: 角色与权限
//! - Memory: 记忆与稳定性
//! - TODO: 任务追踪
//! - LLM: 集成与分解

mod task;
mod intent;
mod agent;
mod ai_agent;
mod memory;
mod config;
mod todo;
mod llm;

pub use task::*;
pub use intent::*;
pub use agent::*;
pub use ai_agent::*;
pub use memory::*;
pub use todo::*;
pub use llm::*;
// Re-export config types (ProviderConfig and ProviderType come from llm/provider)
pub use config::{
    NdcConfig, LlmConfig, OpenAiConfig, AnthropicConfig, MiniMaxConfig,
    OllamaConfig, ReplConfig, RuntimeConfig, StorageConfig,
};

// Re-export commonly used types to avoid conflicts
pub use agent::AgentId;
pub use intent::KnowledgeType;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ===== Task Tests =====

    #[test]
    fn test_task_new() {
        let task = Task::new(
            "Test Task".to_string(),
            "Test Description".to_string(),
            AgentRole::Historian,
        );

        assert_eq!(task.title, "Test Task");
        assert_eq!(task.description, "Test Description");
        assert_eq!(task.state, TaskState::Pending);
        assert_eq!(task.metadata.created_by, AgentRole::Historian);
        assert!(!task.id.to_string().is_empty());
    }

    #[test]
    fn test_task_transition_pending_to_preparing() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        assert!(task.request_transition(TaskState::Preparing).is_ok());
        assert_eq!(task.state, TaskState::Preparing);
    }

    #[test]
    fn test_task_transition_invalid() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        // Cannot transition directly from Pending to InProgress
        let result = task.request_transition(TaskState::InProgress);
        assert!(result.is_err());
        assert_eq!(task.state, TaskState::Pending);
    }

    #[test]
    fn test_task_transition_inprogress_to_verification_or_blocked() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        task.request_transition(TaskState::Preparing).unwrap();
        task.request_transition(TaskState::InProgress).unwrap();

        // Can go to AwaitingVerification or Blocked
        assert!(task.request_transition(TaskState::AwaitingVerification).is_ok());
        assert_eq!(task.state, TaskState::AwaitingVerification);
    }

    #[test]
    fn test_task_priority() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        task.metadata.priority = TaskPriority::High;
        assert_eq!(task.metadata.priority, TaskPriority::High);
    }

    #[test]
    fn test_task_steps() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        let step = ExecutionStep {
            step_id: 1,
            action: Action::ReadFile { path: PathBuf::from("test.rs") },
            status: StepStatus::Completed,
            result: Some(ActionResult {
                success: true,
                output: "content".to_string(),
                error: None,
                metrics: ActionMetrics::default(),
            }),
            executed_at: Some(chrono::Utc::now()),
        };

        task.steps.push(step);
        assert_eq!(task.steps.len(), 1);
        assert_eq!(task.steps[0].status, StepStatus::Completed);
    }

    #[test]
    fn test_task_snapshot() {
        let mut task = Task::new(
            "Test Task".to_string(),
            "Test".to_string(),
            AgentRole::Historian,
        );

        task.capture_worktree_snapshot(
            PathBuf::from("/tmp/worktree"),
            "abc123".to_string(),
            "feature/test".to_string(),
            vec![PathBuf::from("src/main.rs")],
            "Initial commit".to_string(),
        );

        assert_eq!(task.snapshots.len(), 1);
        let snapshot = task.latest_worktree_snapshot().unwrap();
        assert_eq!(snapshot.branch_name, "feature/test");
    }

    #[test]
    fn test_task_state_variants() {
        assert_eq!(TaskState::Pending as u8, 0);
        assert_eq!(TaskState::Preparing as u8, 1);
        assert_eq!(TaskState::InProgress as u8, 2);
        assert_eq!(TaskState::AwaitingVerification as u8, 3);
        assert_eq!(TaskState::Blocked as u8, 4);
        assert_eq!(TaskState::Completed as u8, 5);
        assert_eq!(TaskState::Failed as u8, 6);
        assert_eq!(TaskState::Cancelled as u8, 7);
    }

    // ===== Intent Tests =====

    #[test]
    fn test_intent_new() {
        let intent = Intent {
            id: IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("src/main.rs"),
            },
            effects: vec![],
            reasoning: "Reading main file".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        assert_eq!(intent.agent_role, AgentRole::Implementer);
        assert!(matches!(intent.proposed_action, Action::ReadFile { .. }));
    }

    #[test]
    fn test_action_variants() {
        let actions = vec![
            Action::ReadFile { path: PathBuf::from("test.rs") },
            Action::WriteFile {
                path: PathBuf::from("test.rs"),
                content: "hello".to_string(),
            },
            Action::CreateFile { path: PathBuf::from("new.rs") },
            Action::DeleteFile { path: PathBuf::from("delete.rs") },
            Action::RunCommand {
                command: "echo".to_string(),
                args: vec!["hello".to_string()],
            },
        ];

        assert_eq!(actions.len(), 5);
    }

    #[test]
    fn test_privilege_level_ordering() {
        assert!(PrivilegeLevel::Normal < PrivilegeLevel::Elevated);
        assert!(PrivilegeLevel::Elevated < PrivilegeLevel::High);
        assert!(PrivilegeLevel::High < PrivilegeLevel::Critical);
    }

    #[test]
    fn test_privilege_level_display() {
        assert_eq!(PrivilegeLevel::Normal.to_string(), "Normal");
        assert_eq!(PrivilegeLevel::Elevated.to_string(), "Elevated");
        assert_eq!(PrivilegeLevel::High.to_string(), "High");
        assert_eq!(PrivilegeLevel::Critical.to_string(), "Critical");
    }

    #[test]
    fn test_verdict_allow() {
        let verdict = Verdict::Allow {
            action: Action::ReadFile { path: PathBuf::from("test.rs") },
            privilege: PrivilegeLevel::Normal,
            conditions: vec![],
        };

        match verdict {
            Verdict::Allow { action, privilege, .. } => {
                assert!(matches!(action, Action::ReadFile { .. }));
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[test]
    fn test_verdict_deny() {
        let verdict = Verdict::Deny {
            action: Action::DeleteFile { path: PathBuf::from("test.rs") },
            reason: "Dangerous operation".to_string(),
            error_code: ErrorCode::DangerousOperation,
        };

        match verdict {
            Verdict::Deny { reason, .. } => {
                assert_eq!(reason, "Dangerous operation");
            }
            _ => panic!("Expected Deny verdict"),
        }
    }

    #[test]
    fn test_verdict_require_human() {
        let verdict = Verdict::RequireHuman {
            action: Action::RunCommand {
                command: "rm".to_string(),
                args: vec!["-rf".to_string(), "/".to_string()],
            },
            question: "Are you sure?".to_string(),
            context: HumanContext {
                task_id: None,
                affected_files: vec![],
                risk_level: RiskLevel::Critical,
                alternatives: vec![],
                required_privilege: PrivilegeLevel::Critical,
            },
            timeout: Some(300),
        };

        match verdict {
            Verdict::RequireHuman { question, .. } => {
                assert_eq!(question, "Are you sure?");
            }
            _ => panic!("Expected RequireHuman verdict"),
        }
    }

    #[test]
    fn test_effect_types() {
        let effects = vec![
            Effect::FileOperation {
                path: PathBuf::from("test.rs"),
                operation: FileOp::Write,
            },
            Effect::TaskTransition {
                task_id: TaskId::new(),
                from: TaskState::Pending,
                to: TaskState::Completed,
            },
            Effect::ToolInvocation {
                tool: "git".to_string(),
                args: vec!["status".to_string()],
            },
        ];

        assert_eq!(effects.len(), 3);
    }

    #[test]
    fn test_condition_types() {
        let conditions = vec![
            Condition {
                condition_type: ConditionType::MustPassTests,
                description: "Tests must pass".to_string(),
            },
            Condition {
                condition_type: ConditionType::MustReview,
                description: "Code review required".to_string(),
            },
            Condition {
                condition_type: ConditionType::RequirePrivilege(PrivilegeLevel::High),
                description: "High privilege required".to_string(),
            },
        ];

        assert_eq!(conditions.len(), 3);
    }

    // ===== Agent Tests =====

    #[test]
    fn test_agent_id_new() {
        let id = AgentId::new();
        assert!(!id.0.to_string().is_empty());
    }

    #[test]
    fn test_agent_id_system() {
        let id = AgentId::system();
        assert_eq!(id.0, uuid::Uuid::nil());
    }

    #[test]
    fn test_agent_new() {
        let agent = Agent::new(
            AgentId::new(),
            AgentRole::Implementer,
            "Test Agent".to_string(),
        );

        assert_eq!(agent.role, AgentRole::Implementer);
        assert_eq!(agent.name, "Test Agent");
        assert!(agent.capabilities.is_empty());
    }

    #[test]
    fn test_agent_with_capabilities() {
        let agent = Agent::new(
            AgentId::new(),
            AgentRole::Implementer,
            "Test Agent".to_string(),
        ).with_capabilities(vec!["read".to_string(), "write".to_string()]);

        assert_eq!(agent.capabilities.len(), 2);
        assert!(agent.capabilities.contains(&"read".to_string()));
    }

    #[test]
    fn test_permissions_for_role() {
        let planner_perms = Permissions::for_role(AgentRole::Planner);
        assert!(planner_perms.can_read_files);
        assert!(!planner_perms.can_write_files);

        let admin_perms = Permissions::for_role(AgentRole::Admin);
        assert!(admin_perms.can_delete_files);
        assert!(admin_perms.can_run_commands);
    }

    #[test]
    fn test_agent_roles() {
        let roles = vec![
            AgentRole::Planner,
            AgentRole::Implementer,
            AgentRole::Reviewer,
            AgentRole::Tester,
            AgentRole::Historian,
            AgentRole::Admin,
            AgentRole::Any,
            AgentRole::System,
        ];

        assert_eq!(roles.len(), 8);
    }

    #[test]
    fn test_role_default() {
        let role: AgentRole = AgentRole::default();
        assert_eq!(role, AgentRole::Planner);
    }

    // ===== Memory Tests =====

    #[test]
    fn test_memory_id_new() {
        let id = MemoryId::new();
        assert!(!id.0.to_string().is_empty());
    }

    #[test]
    fn test_memory_stability_ordering() {
        assert!(MemoryStability::Ephemeral < MemoryStability::Derived);
        assert!(MemoryStability::Derived < MemoryStability::Verified);
        assert!(MemoryStability::Verified < MemoryStability::Canonical);
    }

    #[test]
    fn test_memory_stability_values() {
        assert_eq!(MemoryStability::Ephemeral as u8, 0);
        assert_eq!(MemoryStability::Derived as u8, 1);
        assert_eq!(MemoryStability::Verified as u8, 2);
        assert_eq!(MemoryStability::Canonical as u8, 3);
    }

    #[test]
    fn test_memory_query_default() {
        let query = MemoryQuery::default();
        assert!(query.query.is_none());
        assert!(query.stability.is_none());
        assert!(query.tags.is_empty());
    }

    #[test]
    fn test_memory_query_with_filters() {
        let query = MemoryQuery {
            query: Some("test query".to_string()),
            stability: Some(MemoryStability::Verified),
            memory_type: Some("CodeKnowledge".to_string()),
            tags: vec!["rust".to_string(), "testing".to_string()],
            source_task: None,
            min_stability: None,
            max_stability: None,
        };

        assert_eq!(query.query, Some("test query".to_string()));
        assert_eq!(query.stability, Some(MemoryStability::Verified));
        assert_eq!(query.tags.len(), 2);
    }

    #[test]
    fn test_access_control_new() {
        let ac = AccessControl::new(AgentId::new(), MemoryStability::Verified);
        assert!(ac.allow_read(&AgentRole::Implementer));
        assert!(ac.allow_write(&AgentRole::Historian));
    }

    #[test]
    fn test_access_control_stability_levels() {
        // Ephemeral: more roles can write
        let ephemeral = AccessControl::new(AgentId::new(), MemoryStability::Ephemeral);
        assert!(ephemeral.allow_write(&AgentRole::Implementer));

        // Canonical: only Admin can write
        let canonical = AccessControl::new(AgentId::new(), MemoryStability::Canonical);
        assert!(!canonical.allow_write(&AgentRole::Implementer));
        assert!(canonical.allow_write(&AgentRole::Admin));
    }

    #[test]
    fn test_memory_content_variants() {
        let contents = vec![
            MemoryContent::Code(CodeKnowledge {
                file_path: "test.rs".to_string(),
                language: "Rust".to_string(),
                summary: "A test file".to_string(),
                functions: vec![],
            }),
            MemoryContent::ProjectStructure(ProjectStructure {
                root_path: "/tmp".to_string(),
                directories: vec!["src".to_string()],
                important_files: vec!["Cargo.toml".to_string()],
            }),
            MemoryContent::Decision(DecisionRecord {
                decision: "Use Rust".to_string(),
                rationale: "Performance".to_string(),
                alternatives: vec!["C++".to_string()],
                made_by: AgentId::new(),
            }),
            MemoryContent::ErrorSolution(ErrorSolution {
                error: "E0425".to_string(),
                solution: "Define the struct".to_string(),
                prevention: "Type checking".to_string(),
            }),
        ];

        assert_eq!(contents.len(), 4);
    }

    #[test]
    fn test_memory_entry_stability() {
        let entry = MemoryEntry {
            id: MemoryId::new(),
            content: MemoryContent::General {
                text: "test".to_string(),
                metadata: "".to_string(),
            },
            embedding: vec![0.1, 0.2, 0.3],
            relations: vec![],
            metadata: MemoryMetadata {
                stability: MemoryStability::Ephemeral,
                created_at: chrono::Utc::now(),
                created_by: AgentId::new(),
                source_task: TaskId::new(),
                version: 1,
                modified_at: None,
                tags: vec![],
            },
            access_control: AccessControl::new(AgentId::new(), MemoryStability::Ephemeral),
        };

        assert_eq!(*entry.stability(), MemoryStability::Ephemeral);
    }

    #[test]
    fn test_scored_memory() {
        let entry = MemoryEntry {
            id: MemoryId::new(),
            content: MemoryContent::General {
                text: "test".to_string(),
                metadata: "".to_string(),
            },
            embedding: vec![],
            relations: vec![],
            metadata: MemoryMetadata {
                stability: MemoryStability::Derived,
                created_at: chrono::Utc::now(),
                created_by: AgentId::new(),
                source_task: TaskId::new(),
                version: 1,
                modified_at: None,
                tags: vec![],
            },
            access_control: AccessControl::new(AgentId::new(), MemoryStability::Derived),
        };

        let scored = ScoredMemory {
            memory: entry,
            score: 0.95,
        };

        assert_eq!(scored.score, 0.95);
    }

    // ===== Serialization Tests =====

    #[test]
    fn test_task_serialization() {
        let task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            AgentRole::Implementer,
        );

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("Description"));
    }

    #[test]
    fn test_intent_serialization() {
        let intent = Intent {
            id: IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Reviewer,
            proposed_action: Action::RunTests { test_type: TestType::Unit },
            effects: vec![],
            reasoning: "Running unit tests".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&intent).unwrap();
        assert!(json.contains("RunTests"));
    }

    #[test]
    fn test_memory_serialization() {
        let entry = MemoryEntry {
            id: MemoryId::new(),
            content: MemoryContent::Code(CodeKnowledge {
                file_path: "lib.rs".to_string(),
                language: "Rust".to_string(),
                summary: "Library".to_string(),
                functions: vec![],
            }),
            embedding: vec![0.1],
            relations: vec![],
            metadata: MemoryMetadata {
                stability: MemoryStability::Verified,
                created_at: chrono::Utc::now(),
                created_by: AgentId::new(),
                source_task: TaskId::new(),
                version: 1,
                modified_at: None,
                tags: vec!["core".to_string()],
            },
            access_control: AccessControl::new(AgentId::new(), MemoryStability::Verified),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Rust"));
        assert!(json.contains("Verified"));
    }
}
