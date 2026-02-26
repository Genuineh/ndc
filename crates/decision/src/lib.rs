// NDC Decision Engine
//
// Decision & Policy Engine implementation

pub mod engine;
pub mod validators;

pub use engine::*;

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{Action, AgentId, AgentRole, Intent, PrivilegeLevel};
    use std::path::PathBuf;
    use std::sync::Arc;

    // ===== Decision Engine Tests =====

    #[tokio::test]
    async fn test_engine_new() {
        let engine = BasicDecisionEngine::new();
        // Check via policy_state that engine was created
        let policy = engine.policy_state();
        assert!(!policy.strict_mode);
    }

    #[tokio::test]
    async fn test_engine_default() {
        let engine = BasicDecisionEngine::default();
        let policy = engine.policy_state();
        assert!(!policy.strict_mode);
    }

    #[tokio::test]
    async fn test_evaluate_read_file_allowed() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("src/main.rs"),
            },
            effects: vec![],
            reasoning: "Reading source file".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow {
                action, privilege, ..
            } => {
                assert!(matches!(action, Action::ReadFile { .. }));
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_config_file_write_requires_elevated() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: Action::WriteFile {
                path: PathBuf::from("Cargo.toml"),
                content: "[dependencies]".to_string(),
            },
            effects: vec![],
            reasoning: "Updating dependencies".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Elevated);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_delete_file_denied_for_normal_role() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian, // Normal privilege
            proposed_action: Action::DeleteFile {
                path: PathBuf::from("src/main.rs"),
            },
            effects: vec![],
            reasoning: "Deleting file".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Deny {
                reason, error_code, ..
            } => {
                assert!(reason.contains("Insufficient privilege"));
                match error_code {
                    ndc_core::ErrorCode::InsufficientPrivilege { required, granted } => {
                        assert_eq!(required, PrivilegeLevel::High);
                        assert_eq!(granted, PrivilegeLevel::Normal);
                    }
                    _ => panic!("Expected InsufficientPrivilege error code"),
                }
            }
            _ => panic!("Expected Deny verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_dangerous_command_denied() {
        let engine = BasicDecisionEngine::new();

        // Historian has Normal privilege, cannot run dangerous commands
        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian, // Normal privilege
            proposed_action: Action::RunCommand {
                command: "rm -rf /tmp/test".to_string(),
                args: vec![],
            },
            effects: vec![],
            reasoning: "Dangerous operation".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Deny { reason, .. } => {
                assert!(reason.contains("Insufficient privilege"));
            }
            _ => panic!("Expected Deny verdict for dangerous command with Normal privilege"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_build_command_requires_elevated() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: Action::RunCommand {
                command: "cargo build".to_string(),
                args: vec![],
            },
            effects: vec![],
            reasoning: "Building project".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Elevated);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_git_commit_requires_high() {
        let engine = BasicDecisionEngine::new();

        // Admin has Critical privilege, can do git commit (requires High)
        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Admin, // Critical privilege
            proposed_action: Action::Git {
                operation: ndc_core::GitOp::Commit {
                    message: "feat: new feature".to_string(),
                },
            },
            effects: vec![],
            reasoning: "Creating commit".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                // Admin has Critical privilege
                assert_eq!(privilege, PrivilegeLevel::Critical);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_batch() {
        let engine = BasicDecisionEngine::new();

        let intents = vec![
            Intent {
                id: ndc_core::IntentId::new(),
                agent: AgentId::new(),
                agent_role: AgentRole::Historian,
                proposed_action: Action::ReadFile {
                    path: PathBuf::from("test.rs"),
                },
                effects: vec![],
                reasoning: "Reading".to_string(),
                task_id: None,
                timestamp: chrono::Utc::now(),
            },
            Intent {
                id: ndc_core::IntentId::new(),
                agent: AgentId::new(),
                agent_role: AgentRole::Implementer,
                proposed_action: Action::WriteFile {
                    path: PathBuf::from("test.rs"),
                    content: "test".to_string(),
                },
                effects: vec![],
                reasoning: "Writing".to_string(),
                task_id: None,
                timestamp: chrono::Utc::now(),
            },
        ];

        let verdicts = engine.evaluate_batch(intents).await;
        assert_eq!(verdicts.len(), 2);

        match verdicts[0] {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict for ReadFile"),
        }

        match verdicts[1] {
            ndc_core::Verdict::Allow { privilege, .. } => {
                // test.rs is not a config file, so requires Normal privilege
                // Implementer has Elevated, so granted is Elevated
                assert_eq!(privilege, PrivilegeLevel::Elevated);
            }
            _ => panic!("Expected Allow verdict for WriteFile"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_with_validator() {
        struct TestValidator;

        #[async_trait::async_trait]
        impl Validator for TestValidator {
            async fn validate(&self, _intent: &Intent, _policy: &PolicyState) -> ValidationResult {
                ValidationResult::Deny("Test rejection".to_string())
            }

            fn name(&self) -> &str {
                "test_validator"
            }

            fn priority(&self) -> u32 {
                1
            }
        }

        let mut engine = BasicDecisionEngine::new();
        engine.register_validator(Arc::new(TestValidator));

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("test.rs"),
            },
            effects: vec![],
            reasoning: "Reading".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Deny { reason, .. } => {
                assert_eq!(reason, "Test rejection");
            }
            _ => panic!("Expected Deny verdict"),
        }
    }

    #[tokio::test]
    async fn test_policy_state_default() {
        let policy = PolicyState::default();
        assert!(!policy.strict_mode);
        assert!(!policy.allow_dangerous);
        assert_eq!(policy.max_file_modifications, 0);
        assert!(!policy.require_human_for_high_risk); // Default is false
        assert_eq!(policy.human_interventions, 0);
        assert_eq!(policy.denied_intents, 0);
    }

    #[tokio::test]
    async fn test_policy_state_accessors() {
        let engine = BasicDecisionEngine::new();

        let policy = engine.policy_state();
        assert!(!policy.strict_mode);
    }

    #[tokio::test]
    async fn test_evaluate_admin_has_critical_privilege() {
        let engine = BasicDecisionEngine::new();

        // Admin role should be able to read files
        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Admin,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("test.rs"),
            },
            effects: vec![],
            reasoning: "Admin reading".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Critical);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    // ===== Validator Tests =====

    #[tokio::test]
    async fn test_validation_result_types() {
        let results = vec![
            ValidationResult::Allow,
            ValidationResult::Deny("test".to_string()),
            ValidationResult::RequireHuman(
                "question".to_string(),
                ndc_core::HumanContext {
                    task_id: None,
                    affected_files: vec![],
                    risk_level: ndc_core::RiskLevel::Medium,
                    alternatives: vec![],
                    required_privilege: PrivilegeLevel::Normal,
                },
            ),
            ValidationResult::Modify(
                Action::ReadFile {
                    path: PathBuf::from("test.rs"),
                },
                "modified".to_string(),
                vec!["warning1".to_string()],
            ),
            ValidationResult::Defer(
                vec![ndc_core::InformationRequirement {
                    description: "need info".to_string(),
                    source: ndc_core::InformationSource::Human,
                }],
                Some(60),
            ),
        ];

        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_write_file_adds_test_condition() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Implementer,
            proposed_action: Action::WriteFile {
                path: PathBuf::from("src/lib.rs"),
                content: "fn main() {}".to_string(),
            },
            effects: vec![],
            reasoning: "Writing code".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { conditions, .. } => {
                assert!(!conditions.is_empty());
                assert!(
                    conditions.iter().any(|c| matches!(
                        c.condition_type,
                        ndc_core::ConditionType::MustPassTests
                    ))
                );
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_create_task_allowed() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Planner,
            proposed_action: Action::CreateTask {
                task_spec: ndc_core::TaskSpec {
                    title: "Test Task".to_string(),
                    description: "A test".to_string(),
                    task_type: "feature".to_string(),
                },
            },
            effects: vec![],
            reasoning: "Creating a task".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_read_only_action_types() {
        let engine = BasicDecisionEngine::new();

        // Test SearchKnowledge
        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian,
            proposed_action: Action::SearchKnowledge {
                query: "test query".to_string(),
            },
            effects: vec![],
            reasoning: "Searching".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;
        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_verdict_modify() {
        // Validator that modifies action
        struct ModifyValidator;

        #[async_trait::async_trait]
        impl Validator for ModifyValidator {
            async fn validate(&self, _intent: &Intent, _policy: &PolicyState) -> ValidationResult {
                ValidationResult::Modify(
                    Action::ReadFile {
                        path: PathBuf::from("modified.rs"),
                    },
                    "Modified for safety".to_string(),
                    vec!["Warning: path changed".to_string()],
                )
            }

            fn name(&self) -> &str {
                "modify_validator"
            }
            fn priority(&self) -> u32 {
                1
            }
        }

        let mut engine = BasicDecisionEngine::new();
        engine.register_validator(Arc::new(ModifyValidator));

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("original.rs"),
            },
            effects: vec![],
            reasoning: "Original".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Modify {
                modified_action,
                reason,
                ..
            } => {
                match modified_action {
                    Action::ReadFile { path } => {
                        assert_eq!(path, PathBuf::from("modified.rs"));
                    }
                    _ => panic!("Expected modified ReadFile action"),
                }
                assert_eq!(reason, "Modified for safety");
            }
            _ => panic!("Expected Modify verdict"),
        }
    }

    #[tokio::test]
    async fn test_verdict_defer() {
        struct DeferValidator;

        #[async_trait::async_trait]
        impl Validator for DeferValidator {
            async fn validate(&self, _intent: &Intent, _policy: &PolicyState) -> ValidationResult {
                ValidationResult::Defer(
                    vec![ndc_core::InformationRequirement {
                        description: "Need more context".to_string(),
                        source: ndc_core::InformationSource::Human,
                    }],
                    Some(120),
                )
            }

            fn name(&self) -> &str {
                "defer_validator"
            }
            fn priority(&self) -> u32 {
                1
            }
        }

        let mut engine = BasicDecisionEngine::new();
        engine.register_validator(Arc::new(DeferValidator));

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian,
            proposed_action: Action::ReadFile {
                path: PathBuf::from("test.rs"),
            },
            effects: vec![],
            reasoning: "Test".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Defer {
                required_info,
                retry_after,
                ..
            } => {
                assert_eq!(required_info.len(), 1);
                assert_eq!(retry_after, Some(120));
            }
            _ => panic!("Expected Defer verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_run_tests() {
        let engine = BasicDecisionEngine::new();

        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Tester,
            proposed_action: Action::RunTests {
                test_type: ndc_core::TestType::All,
            },
            effects: vec![],
            reasoning: "Running all tests".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Allow { privilege, .. } => {
                assert_eq!(privilege, PrivilegeLevel::Normal);
            }
            _ => panic!("Expected Allow verdict"),
        }
    }

    #[tokio::test]
    async fn test_evaluate_save_knowledge_requires_elevated() {
        let engine = BasicDecisionEngine::new();

        // Historian has Normal privilege, but SaveKnowledge requires Elevated
        // So this should be denied
        let intent = Intent {
            id: ndc_core::IntentId::new(),
            agent: AgentId::new(),
            agent_role: AgentRole::Historian, // Normal privilege
            proposed_action: Action::SaveKnowledge {
                knowledge: ndc_core::KnowledgeSpec {
                    title: "New Knowledge".to_string(),
                    content: "Content".to_string(),
                    knowledge_type: ndc_core::KnowledgeType::Documentation,
                },
            },
            effects: vec![],
            reasoning: "Saving knowledge".to_string(),
            task_id: None,
            timestamp: chrono::Utc::now(),
        };

        let verdict = engine.evaluate(intent).await;

        match verdict {
            ndc_core::Verdict::Deny { reason, .. } => {
                assert!(reason.contains("Insufficient privilege"));
            }
            _ => panic!("Expected Deny verdict for Historian saving knowledge"),
        }
    }
}
