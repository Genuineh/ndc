//! TODO Mapping Service - Associate user intents with TODO items
//!
//! Responsibilities:
//! - Map user intents to existing TODO items
//! - Find related TODOs based on context
//! - Create TODO chains from user requirements
//! - Track TODO relationships and dependencies

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// User intent for TODO mapping
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIntent {
    /// Original user request
    pub request: String,
    /// Extracted action (e.g., "add", "fix", "implement", "refactor")
    pub action: String,
    /// Target entity (e.g., "feature", "bug", "test", "documentation")
    pub target: String,
    /// Specific subject (e.g., "login", "api", "database")
    pub subject: Option<String>,
    /// Priority extracted from intent
    pub priority: IntentPriority,
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Intent priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentPriority {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

/// A TODO item for mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Unique TODO ID
    pub id: String,
    /// TODO title
    pub title: String,
    /// TODO description
    pub description: String,
    /// Status
    pub status: TodoStatus,
    /// Priority
    pub priority: TodoPriority,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Related file paths
    pub related_files: Vec<PathBuf>,
    /// Parent TODO ID (for sub-tasks)
    pub parent: Option<String>,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Updated timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// TODO status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

/// TODO priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoPriority {
    P0, // Critical
    P1, // High
    P2, // Medium
    P3, // Low
}

/// Mapping result
#[derive(Debug, Clone)]
pub struct MappingResult {
    /// The original intent
    pub intent: UserIntent,
    /// Matched TODO items
    pub matched_todos: Vec<TodoItem>,
    /// Newly created TODOs
    pub created_todos: Vec<TodoItem>,
    /// Suggestions for related work
    pub suggestions: Vec<String>,
    /// Whether mapping was successful
    pub success: bool,
}

/// TODO Mapping Service
///
/// Maps user intents to TODO items and manages TODO creation.
#[derive(Debug)]
pub struct TodoMappingService {
    /// TODO storage
    todos: Arc<RwLock<HashMap<String, TodoItem>>>,
    /// Mapping configurations
    config: MappingConfig,
}

/// Mapping configuration
#[derive(Debug, Clone)]
pub struct MappingConfig {
    /// Minimum similarity threshold (0.0 - 1.0)
    similarity_threshold: f32,
    /// Max related TODOs to return
    max_related: usize,
    /// Auto-create TODOs for unmatched intents
    auto_create: bool,
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.6,
            max_related: 5,
            auto_create: true,
        }
    }
}

/// Calculate string similarity (simple Jaccard index)
fn calculate_similarity(s1: &str, s2: &str) -> f32 {
    let set1: std::collections::HashSet<String> =
        s1.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
    let set2: std::collections::HashSet<String> =
        s2.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();

    if set1.is_empty() || set2.is_empty() {
        return 0.0;
    }

    let intersection: std::collections::HashSet<String> =
        set1.intersection(&set2).cloned().collect();
    let union: std::collections::HashSet<String> =
        set1.union(&set2).cloned().collect();

    intersection.len() as f32 / union.len() as f32
}

impl TodoMappingService {
    /// Create a new mapping service
    pub fn new(config: Option<MappingConfig>) -> Self {
        let cfg = config.unwrap_or_default();
        Self {
            todos: Arc::new(RwLock::new(HashMap::new())),
            config: cfg,
        }
    }

    /// Parse a user intent from a request
    pub fn parse_intent(request: &str) -> UserIntent {
        let request_lower = request.to_lowercase();

        // Extract action
        let action = Self::extract_action(&request_lower);

        // Extract target
        let target = Self::extract_target(&request_lower);

        // Extract subject
        let subject = Self::extract_subject(&request_lower);

        // Extract priority
        let priority = Self::extract_priority(&request_lower);

        UserIntent {
            request: request.to_string(),
            action,
            target,
            subject,
            priority,
            context: HashMap::new(),
        }
    }

    /// Extract action from request
    fn extract_action(request: &str) -> String {
        let action_patterns = [
            ("add", vec!["add", "create", "implement", "introduce", "new"]),
            ("fix", vec!["fix", "repair", "resolve", "correct", "bug"]),
            ("update", vec!["update", "modify", "change", "improve", "enhance"]),
            ("remove", vec!["remove", "delete", "eliminate", "drop"]),
            ("refactor", vec!["refactor", "restructure", "rewrite", "reorganize"]),
            ("test", vec!["test", "verify", "validate", "check"]),
            ("document", vec!["document", "docs", "comment", "explain"]),
            ("review", vec!["review", "examine", "analyze", "audit"]),
        ];

        for (action, patterns) in action_patterns.iter() {
            for pattern in patterns {
                if request.contains(pattern) {
                    return action.to_string();
                }
            }
        }

        "unknown".to_string()
    }

    /// Extract target from request
    fn extract_target(request: &str) -> String {
        let target_patterns = [
            ("feature", vec!["feature", "functionality", "capability"]),
            ("bug", vec!["bug", "error", "issue", "problem", "crash"]),
            ("test", vec!["test", "testing", "unit test", "integration test"]),
            ("api", vec!["api", "endpoint", "interface"]),
            ("database", vec!["database", "db", "storage", "query"]),
            ("ui", vec!["ui", "interface", "button", "screen", "page"]),
            ("documentation", vec!["docs", "documentation", "readme"]),
            ("performance", vec!["performance", "speed", "optimize", "fast"]),
            ("security", vec!["security", "auth", "permission", "secure"]),
            ("config", vec!["config", "configuration", "setting"]),
        ];

        for (target, patterns) in target_patterns.iter() {
            for pattern in patterns {
                if request.contains(pattern) {
                    return target.to_string();
                }
            }
        }

        "general".to_string()
    }

    /// Extract subject from request
    fn extract_subject(request: &str) -> Option<String> {
        // Try to extract a noun phrase after action words
        let words: Vec<&str> = request.split_whitespace().collect();

        // Common subject indicators
        let subject_indicators = ["the", "a", "an", "for", "to"];

        for (i, word) in words.iter().enumerate() {
            // Skip action words and indicators
            if ["add", "fix", "update", "remove", "implement"].contains(word) {
                if i + 1 < words.len() {
                    let next_word = words[i + 1];
                    if !subject_indicators.contains(&next_word) && next_word.len() > 2 {
                        return Some(next_word.to_string());
                    }
                }
            }
        }

        None
    }

    /// Extract priority from request
    fn extract_priority(request: &str) -> IntentPriority {
        let priority_patterns = [
            (IntentPriority::Critical, vec!["critical", "urgent", "asap", "immediately", "emergency"]),
            (IntentPriority::High, vec!["important", "high priority", "soon"]),
            (IntentPriority::Medium, vec!["moderate", "normal", "when possible"]),
            (IntentPriority::Low, vec!["low priority", "eventually", "nice to have"]),
        ];

        for (priority, patterns) in priority_patterns.iter() {
            for pattern in patterns {
                if request.contains(pattern) {
                    return *priority;
                }
            }
        }

        IntentPriority::Medium // Default priority
    }

    /// Map an intent to existing TODOs
    pub async fn map_intent(&self, intent: &UserIntent) -> MappingResult {
        let todos = self.todos.read().unwrap();
        let todos_vec: Vec<TodoItem> = todos.values().cloned().collect();

        // Find matching TODOs
        let matched_todos: Vec<TodoItem> = todos_vec
            .iter()
            .filter(|todo| self.matches_intent(todo, intent))
            .cloned()
            .collect();

        let mut result = MappingResult {
            intent: intent.clone(),
            matched_todos: matched_todos.clone(),
            created_todos: vec![],
            suggestions: vec![],
            success: !matched_todos.is_empty(),
        };

        // If no matches and auto-create is enabled, create a new TODO
        if result.matched_todos.is_empty() && self.config.auto_create {
            drop(todos); // Release lock
            let new_todo = self.create_todo_from_intent(intent);
            result.created_todos.push(new_todo);
            result.success = true;
        }

        // Generate suggestions
        result.suggestions = self.generate_suggestions(intent, &matched_todos);

        result
    }

    /// Check if a TODO matches an intent
    fn matches_intent(&self, todo: &TodoItem, intent: &UserIntent) -> bool {
        // Check title similarity
        let title_sim = calculate_similarity(&todo.title, &intent.request);

        // Check tag matching
        let action_tag = format!("action:{}", intent.action);
        let target_tag = format!("target:{}", intent.target);

        let tag_match = todo.tags.contains(&action_tag) || todo.tags.contains(&target_tag);

        // Check if TODO is pending
        let is_pending = matches!(todo.status, TodoStatus::Pending);

        title_sim >= self.config.similarity_threshold || (tag_match && is_pending)
    }

    /// Create a TODO from an intent
    fn create_todo_from_intent(&self, intent: &UserIntent) -> TodoItem {
        let now = chrono::Utc::now();
        let id = format!("todo-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());

        let title = format!("{} {} {}", intent.action, intent.target, intent.subject.clone().unwrap_or_default());

        let tags = vec![
            format!("action:{}", intent.action),
            format!("target:{}", intent.target),
            format!("priority:{:?}", intent.priority),
        ];

        let priority = match intent.priority {
            IntentPriority::Critical => TodoPriority::P0,
            IntentPriority::High => TodoPriority::P1,
            IntentPriority::Medium => TodoPriority::P2,
            IntentPriority::Low => TodoPriority::P3,
            IntentPriority::Unknown => TodoPriority::P3,
        };

        TodoItem {
            id,
            title,
            description: intent.request.clone(),
            status: TodoStatus::Pending,
            priority,
            tags,
            related_files: vec![],
            parent: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Generate suggestions for related work
    fn generate_suggestions(&self, intent: &UserIntent, matched_todos: &[TodoItem]) -> Vec<String> {
        let mut suggestions = vec![];

        // Suggest documentation if implementing a feature
        if intent.action == "implement" || intent.action == "add" {
            suggestions.push("Consider adding tests for the new functionality".to_string());
            suggestions.push("Update documentation to reflect changes".to_string());
        }

        // Suggest fixes for bugs
        if intent.target == "bug" {
            suggestions.push("Add a test to prevent regression".to_string());
            suggestions.push("Check for similar issues in related code".to_string());
        }

        // Suggest checking related TODOs
        if !matched_todos.is_empty() {
            suggestions.push(format!("There are {} related TODO(s) to consider", matched_todos.len()));
        }

        suggestions
    }

    /// Find related TODOs for a TODO
    pub async fn find_related_todos(&self, todo_id: &str) -> Vec<TodoItem> {
        let todos = self.todos.read().unwrap();

        if let Some(todo) = todos.get(todo_id) {
            todos
                .values()
                .filter(|t| t.id != todo_id)
                .filter(|t| {
                    // Check tag overlap
                    let tag_overlap = t.tags.iter().any(|tag| todo.tags.contains(tag));
                    // Check related files
                    let file_overlap = t.related_files.iter().any(|f| todo.related_files.contains(f));

                    tag_overlap || file_overlap
                })
                .cloned()
                .take(self.config.max_related)
                .collect()
        } else {
            vec![]
        }
    }

    /// Create a TODO chain from intents
    pub async fn create_todo_chain(&self, intents: &[UserIntent]) -> Vec<TodoItem> {
        let mut chain = vec![];
        let mut parent_id = None;

        for intent in intents {
            let mut todo = self.create_todo_from_intent(intent);
            todo.parent = parent_id;
            parent_id = Some(todo.id.clone());
            chain.push(todo);
        }

        // Store all TODOs
        {
            let mut todos = self.todos.write().unwrap();
            for t in &chain {
                todos.insert(t.id.clone(), t.clone());
            }
        }

        chain
    }

    /// Add a TODO to the service
    pub fn add_todo(&self, todo: TodoItem) {
        let mut todos = self.todos.write().unwrap();
        todos.insert(todo.id.clone(), todo);
    }

    /// Get a TODO by ID
    pub fn get_todo(&self, id: &str) -> Option<TodoItem> {
        let todos = self.todos.read().unwrap();
        todos.get(id).cloned()
    }

    /// List all TODOs
    pub fn list_todos(&self) -> Vec<TodoItem> {
        let todos = self.todos.read().unwrap();
        todos.values().cloned().collect()
    }

    /// Update a TODO
    pub fn update_todo(&self, id: &str, updates: TodoUpdate) -> Option<TodoItem> {
        let mut todos = self.todos.write().unwrap();

        if let Some(todo) = todos.get_mut(id) {
            if let Some(status) = updates.status {
                todo.status = status;
            }
            if let Some(priority) = updates.priority {
                todo.priority = priority;
            }
            if let Some(description) = updates.description {
                todo.description = description;
            }
            todo.updated_at = chrono::Utc::now();

            Some(todo.clone())
        } else {
            None
        }
    }
}

/// TODO update fields
#[derive(Debug, Default)]
pub struct TodoUpdate {
    pub status: Option<TodoStatus>,
    pub priority: Option<TodoPriority>,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_intent_add_feature() {
        let intent = TodoMappingService::parse_intent("Add a new login feature");

        assert_eq!(intent.action, "add");
        assert_eq!(intent.target, "feature");
        assert_eq!(intent.priority, IntentPriority::Medium);
    }

    #[test]
    fn test_parse_intent_fix_bug() {
        let intent = TodoMappingService::parse_intent("Fix the database connection bug urgently");

        assert_eq!(intent.action, "fix");
        assert_eq!(intent.target, "bug");
        assert_eq!(intent.priority, IntentPriority::Critical);
    }

    #[test]
    fn test_parse_intent_with_priority() {
        let intent = TodoMappingService::parse_intent("Update the API documentation - high priority");

        assert_eq!(intent.action, "update");
        assert_eq!(intent.target, "api");
        assert_eq!(intent.priority, IntentPriority::High);
    }

    #[test]
    fn test_similarity() {
        let sim1 = calculate_similarity("add login feature", "add login feature");
        assert!((sim1 - 1.0).abs() < 0.01);

        let sim2 = calculate_similarity("add login feature", "fix database bug");
        assert!(sim2 < 0.5);
    }

    #[tokio::test]
    async fn test_map_intent_creates_todo() {
        let service = TodoMappingService::new(None);

        let intent = TodoMappingService::parse_intent("Add user authentication");
        let result = service.map_intent(&intent).await;

        assert!(result.success);
        assert!(result.matched_todos.is_empty());
        assert_eq!(result.created_todos.len(), 1);

        let created = &result.created_todos[0];
        assert_eq!(created.title, "add security user");
        assert_eq!(created.status, TodoStatus::Pending);
    }

    #[tokio::test]
    async fn test_find_related_todos() {
        let service = TodoMappingService::new(None);

        // Add some TODOs
        let todo1 = TodoItem {
            id: "todo-1".to_string(),
            title: "Add login feature".to_string(),
            description: "Add user login".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::P1,
            tags: vec!["action:add".to_string(), "target:feature".to_string()],
            related_files: vec![],
            parent: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let todo2 = TodoItem {
            id: "todo-2".to_string(),
            title: "Add logout feature".to_string(),
            description: "Add user logout".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::P2,
            tags: vec!["action:add".to_string(), "target:feature".to_string()],
            related_files: vec![],
            parent: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        service.add_todo(todo1);
        service.add_todo(todo2);

        let related = service.find_related_todos("todo-1").await;
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].id, "todo-2");
    }

    #[tokio::test]
    async fn test_create_todo_chain() {
        let service = TodoMappingService::new(None);

        let intents = vec![
            TodoMappingService::parse_intent("Add login feature"),
            TodoMappingService::parse_intent("Add logout feature"),
            TodoMappingService::parse_intent("Add password reset"),
        ];

        let chain = service.create_todo_chain(&intents).await;

        assert_eq!(chain.len(), 3);
        assert!(chain[0].parent.is_none());
        assert_eq!(chain[1].parent, Some(chain[0].id.clone()));
        assert_eq!(chain[2].parent, Some(chain[1].id.clone()));
    }
}
