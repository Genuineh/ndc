//! SQLite Storage - Persistent storage using SQLite
//!
//! Provides persistent task and memory storage using SQLite database
//!
//! Features:
//! - Persistent storage across sessions
//! - Indexed task lookups
//! - Automatic schema migration
//! - Async-friendly using spawn_blocking

use ndc_core::{Task, TaskId, MemoryEntry, MemoryId};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;
use rusqlite::{self, OptionalExtension};
#[cfg(feature = "sqlite")]
use uuid;

/// SQLite storage error
#[derive(Debug, thiserror::Error)]
pub enum SqliteStorageError {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),

    #[error("Invalid task data: {0}")]
    InvalidData(String),
}

/// SQLite storage implementation
#[derive(Debug, Clone)]
pub struct SqliteStorage {
    /// Database file path
    path: PathBuf,
}

impl SqliteStorage {
    /// Create a new SQLite storage with the given database path
    pub async fn new(path: PathBuf) -> Result<Self, SqliteStorageError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| SqliteStorageError::DatabaseError(e.to_string()))?;
        }

        // Verify we can open the database and initialize schema
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path_clone)
                .map_err(|e| SqliteStorageError::DatabaseError(e.to_string()))?;
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(|e| SqliteStorageError::DatabaseError(e.to_string()))?;
            Self::init_schema(&conn)?;
            Ok::<_, SqliteStorageError>(())
        })
        .await
        .map_err(|e| SqliteStorageError::DatabaseError(e.to_string()))??;

        info!("SQLite storage initialized at: {:?}", path);

        Ok(Self { path })
    }

    /// Initialize database schema
    fn init_schema(conn: &rusqlite::Connection) -> Result<(), SqliteStorageError> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'Pending',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                created_by TEXT NOT NULL,
                priority TEXT NOT NULL DEFAULT 'Medium',
                metadata TEXT NOT NULL,
                steps TEXT NOT NULL DEFAULT '[]',
                intent TEXT,
                verdict TEXT,
                quality_gate TEXT,
                snapshots TEXT NOT NULL DEFAULT '[]',
                lightweight_snapshots TEXT NOT NULL DEFAULT '[]'
            )
            "#,
            [],
        ).map_err(|e| SqliteStorageError::MigrationError(e.to_string()))?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding TEXT NOT NULL DEFAULT '[]',
                relations TEXT NOT NULL DEFAULT '[]',
                metadata TEXT NOT NULL,
                access_control TEXT NOT NULL
            )
            "#,
            [],
        ).map_err(|e| SqliteStorageError::MigrationError(e.to_string()))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_state ON tasks(state)",
            [],
        ).map_err(|e| SqliteStorageError::MigrationError(e.to_string()))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_created_at ON tasks(created_at)",
            [],
        ).map_err(|e| SqliteStorageError::MigrationError(e.to_string()))?;

        Ok(())
    }

    /// Get the database path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Helper function to run blocking SQLite operations
async fn run_sqlite<T, F>(path: PathBuf, f: F) -> Result<T, String>
where
    F: FnOnce(&rusqlite::Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&path)
            .map_err(|e| e.to_string())?;
        f(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[async_trait]
impl super::Storage for SqliteStorage {
    async fn save_task(&self, task: &Task) -> Result<(), String> {
        let path = self.path.clone();

        // Serialize all complex types before the async block
        let task_id = task.id.to_string();
        let title = task.title.clone();
        let description = task.description.clone();
        let state = serde_json::to_string(&task.state)
            .map_err(|e| e.to_string())?;
        let created_at = task.metadata.created_at.to_rfc3339();
        let updated_at = task.metadata.updated_at.to_rfc3339();
        let created_by = serde_json::to_string(&task.metadata.created_by)
            .map_err(|e| e.to_string())?;
        let priority = serde_json::to_string(&task.metadata.priority)
            .map_err(|e| e.to_string())?;
        let metadata = serde_json::to_string(&task.metadata)
            .map_err(|e| e.to_string())?;
        let steps = serde_json::to_string(&task.steps)
            .map_err(|e| e.to_string())?;
        let snapshots = serde_json::to_string(&task.snapshots)
            .map_err(|e| e.to_string())?;
        let lightweight_snapshots = serde_json::to_string(&task.lightweight_snapshots)
            .map_err(|e| e.to_string())?;
        let intent = task.intent.as_ref()
            .map(|i| serde_json::to_string(i).map_err(|e| e.to_string()))
            .transpose()?;
        let verdict = task.verdict.as_ref()
            .map(|v| serde_json::to_string(v).map_err(|e| e.to_string()))
            .transpose()?;
        let quality_gate = task.quality_gate.as_ref()
            .map(|g| serde_json::to_string(g).map_err(|e| e.to_string()))
            .transpose()?;

        run_sqlite(path, move |conn| {
            conn.execute(
                r#"
                INSERT INTO tasks (
                    id, title, description, state, created_at, updated_at,
                    created_by, priority, metadata, steps, intent, verdict,
                    quality_gate, snapshots, lightweight_snapshots
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    state = excluded.state,
                    updated_at = excluded.updated_at,
                    metadata = excluded.metadata,
                    steps = excluded.steps,
                    snapshots = excluded.snapshots,
                    lightweight_snapshots = excluded.lightweight_snapshots
                "#,
                rusqlite::params![
                    task_id, title, description, state, created_at, updated_at,
                    created_by, priority, metadata, steps, intent, verdict,
                    quality_gate, snapshots, lightweight_snapshots,
                ],
            )
            .map_err(|e| e.to_string())
        }).await?;

        Ok(())
    }

    async fn get_task(&self, task_id: &TaskId) -> Result<Option<Task>, String> {
        let path = self.path.clone();
        let task_id_str = task_id.to_string();

        run_sqlite(path, move |conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, title, description, state, created_at, updated_at,
                       created_by, priority, metadata, steps, intent, verdict,
                       quality_gate, snapshots, lightweight_snapshots
                FROM tasks WHERE id = ?
                "#,
            ).map_err(|e| e.to_string())?;

            let task_opt = stmt.query_row([&task_id_str], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let description: String = row.get(2)?;
                let state: String = row.get(3)?;
                let _created_at: String = row.get(4)?;
                let _updated_at: String = row.get(5)?;
                let _created_by: String = row.get(6)?;
                let _priority: String = row.get(7)?;
                let metadata_json: String = row.get(8)?;
                let steps_json: String = row.get(9)?;
                let intent_json: Option<String> = row.get(10)?;
                let verdict_json: Option<String> = row.get(11)?;
                let quality_gate_json: Option<String> = row.get(12)?;
                let snapshots_json: String = row.get(13)?;
                let lightweight_snapshots_json: String = row.get(14)?;

                let task_id_parsed: TaskId = id.parse().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let state_parsed: ndc_core::TaskState = serde_json::from_str(&state).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let metadata: ndc_core::TaskMetadata = serde_json::from_str(&metadata_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let steps: Vec<ndc_core::ExecutionStep> = serde_json::from_str(&steps_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let intent = intent_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let verdict = verdict_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let quality_gate = quality_gate_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let snapshots: Vec<ndc_core::GitWorktreeSnapshot> = serde_json::from_str(&snapshots_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let lightweight_snapshots: Vec<ndc_core::LightweightSnapshot> = serde_json::from_str(&lightweight_snapshots_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;

                Ok(Task {
                    id: task_id_parsed,
                    title,
                    description,
                    state: state_parsed,
                    allowed_transitions: Vec::new(),
                    steps,
                    quality_gate,
                    snapshots,
                    lightweight_snapshots,
                    metadata,
                    intent,
                    verdict,
                })
            }).optional();

            task_opt.map_err(|e| e.to_string())
        }).await
    }

    async fn list_tasks(&self) -> Result<Vec<Task>, String> {
        let path = self.path.clone();

        run_sqlite(path, move |conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, title, description, state, created_at, updated_at,
                       created_by, priority, metadata, steps, intent, verdict,
                       quality_gate, snapshots, lightweight_snapshots
                FROM tasks ORDER BY created_at DESC
                "#,
            ).map_err(|e| e.to_string())?;

            let mut tasks: Vec<Task> = Vec::new();
            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let description: String = row.get(2)?;
                let state: String = row.get(3)?;
                let metadata_json: String = row.get(8)?;
                let steps_json: String = row.get(9)?;
                let intent_json: Option<String> = row.get(10)?;
                let verdict_json: Option<String> = row.get(11)?;
                let quality_gate_json: Option<String> = row.get(12)?;
                let snapshots_json: String = row.get(13)?;
                let lightweight_snapshots_json: String = row.get(14)?;

                let task_id_parsed: TaskId = id.parse().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let state_parsed: ndc_core::TaskState = serde_json::from_str(&state).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let metadata: ndc_core::TaskMetadata = serde_json::from_str(&metadata_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let steps: Vec<ndc_core::ExecutionStep> = serde_json::from_str(&steps_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let intent = intent_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let verdict = verdict_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let quality_gate = quality_gate_json.map(|s| serde_json::from_str(&s))
                    .transpose().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let snapshots: Vec<ndc_core::GitWorktreeSnapshot> = serde_json::from_str(&snapshots_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let lightweight_snapshots: Vec<ndc_core::LightweightSnapshot> = serde_json::from_str(&lightweight_snapshots_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;

                Ok(Task {
                    id: task_id_parsed,
                    title,
                    description,
                    state: state_parsed,
                    allowed_transitions: Vec::new(),
                    steps,
                    quality_gate,
                    snapshots,
                    lightweight_snapshots,
                    metadata,
                    intent,
                    verdict,
                })
            }).map_err(|e| e.to_string())?;

            let mut result = Vec::new();
            for task in rows {
                result.push(task.map_err(|e| e.to_string())?);
            }
            Ok(result)
        }).await
    }

    async fn save_memory(&self, memory: &MemoryEntry) -> Result<(), String> {
        let path = self.path.clone();

        let memory_id = memory.id.0.to_string();
        let content = serde_json::to_string(&memory.content)
            .map_err(|e| e.to_string())?;
        let embedding = serde_json::to_string(&memory.embedding)
            .map_err(|e| e.to_string())?;
        let relations = serde_json::to_string(&memory.relations)
            .map_err(|e| e.to_string())?;
        let metadata = serde_json::to_string(&memory.metadata)
            .map_err(|e| e.to_string())?;
        let access_control = serde_json::to_string(&memory.access_control)
            .map_err(|e| e.to_string())?;

        run_sqlite(path, move |conn| {
            conn.execute(
                r#"
                INSERT INTO memories (
                    id, content, embedding, relations, metadata, access_control
                ) VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    content = excluded.content,
                    embedding = excluded.embedding,
                    relations = excluded.relations,
                    metadata = excluded.metadata,
                    access_control = excluded.access_control
                "#,
                rusqlite::params![
                    memory_id, content, embedding, relations, metadata, access_control,
                ],
            )
            .map_err(|e| e.to_string())
        }).await?;

        Ok(())
    }

    async fn get_memory(&self, memory_id: &MemoryId) -> Result<Option<MemoryEntry>, String> {
        let path = self.path.clone();
        let memory_id_str = memory_id.0.to_string();

        run_sqlite(path, move |conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, content, embedding, relations, metadata, access_control
                FROM memories WHERE id = ?
                "#,
            ).map_err(|e| e.to_string())?;

            let memory_opt = stmt.query_row([&memory_id_str], |row| {
                let id: String = row.get(0)?;
                let content_json: String = row.get(1)?;
                let embedding_json: String = row.get(2)?;
                let relations_json: String = row.get(3)?;
                let metadata_json: String = row.get(4)?;
                let access_control_json: String = row.get(5)?;

                let memory_id_parsed: uuid::Uuid = id.parse().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let content: ndc_core::MemoryContent = serde_json::from_str(&content_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let embedding: Vec<f32> = serde_json::from_str(&embedding_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let relations: Vec<ndc_core::Relation> = serde_json::from_str(&relations_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let metadata: ndc_core::MemoryMetadata = serde_json::from_str(&metadata_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;
                let access_control: ndc_core::AccessControl = serde_json::from_str(&access_control_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
                })?;

                Ok(MemoryEntry {
                    id: MemoryId(memory_id_parsed),
                    content,
                    embedding,
                    relations,
                    metadata,
                    access_control,
                })
            }).optional();

            memory_opt.map_err(|e| e.to_string())
        }).await
    }
}

/// Create a new shared SQLite storage
pub async fn create_sqlite_storage(path: PathBuf) -> Result<Arc<SqliteStorage>, SqliteStorageError> {
    let storage = SqliteStorage::new(path).await?;
    Ok(Arc::new(storage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use ndc_core::{TaskState, TaskMetadata, MemoryContent};
    use tempfile::tempdir;
    use ulid::Ulid;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_sqlite_storage_new() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();
        assert!(storage.path().exists());
    }

    #[tokio::test]
    async fn test_sqlite_storage_save_and_get_task() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();

        // Create a test task
        let task = Task {
            id: Ulid::new().into(),
            title: "Test Task".to_string(),
            description: "Test Description".to_string(),
            state: TaskState::Pending,
            allowed_transitions: vec![TaskState::InProgress],
            steps: vec![],
            quality_gate: None,
            snapshots: vec![],
            lightweight_snapshots: vec![],
            metadata: TaskMetadata::default(),
            intent: None,
            verdict: None,
        };

        // Save the task
        storage.save_task(&task).await.unwrap();

        // Retrieve the task
        let retrieved = storage.get_task(&task.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.title, task.title);
        assert_eq!(retrieved.description, task.description);
        assert_eq!(retrieved.state, task.state);
    }

    #[tokio::test]
    async fn test_sqlite_storage_list_tasks() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();

        // Create and save multiple tasks
        for i in 0..3 {
            let task = Task {
                id: Ulid::new().into(),
                title: format!("Test Task {}", i),
                description: format!("Description {}", i),
                state: TaskState::Pending,
                allowed_transitions: vec![],
                steps: vec![],
                quality_gate: None,
                snapshots: vec![],
                lightweight_snapshots: vec![],
                metadata: TaskMetadata::default(),
                intent: None,
                verdict: None,
            };
            storage.save_task(&task).await.unwrap();
        }

        // List tasks
        let tasks = storage.list_tasks().await.unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[tokio::test]
    async fn test_sqlite_storage_get_nonexistent_task() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();

        let non_existent_id: TaskId = Ulid::new().into();
        let result = storage.get_task(&non_existent_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_storage_save_and_get_memory() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();

        // Create a test memory
        let memory = MemoryEntry {
            id: MemoryId(Uuid::new_v4()),
            content: MemoryContent::General {
                text: "Test observation".to_string(),
                metadata: "test".to_string(),
            },
            embedding: vec![0.1, 0.2, 0.3],
            relations: vec![],
            metadata: ndc_core::MemoryMetadata {
                stability: ndc_core::MemoryStability::Ephemeral,
                created_at: chrono::Utc::now(),
                created_by: ndc_core::AgentId(uuid::Uuid::new_v4()),
                source_task: Ulid::new().into(),
                version: 1,
                modified_at: None,
                tags: vec![],
            },
            access_control: ndc_core::AccessControl::new(
                ndc_core::AgentId(uuid::Uuid::new_v4()),
                ndc_core::MemoryStability::Ephemeral,
            ),
        };

        // Save the memory
        storage.save_memory(&memory).await.unwrap();

        // Retrieve the memory
        let retrieved = storage.get_memory(&memory.id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        match &retrieved.content {
            MemoryContent::General { text, .. } => {
                assert_eq!(text, "Test observation");
            }
            _ => panic!("Expected General content"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_storage_task_update() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let storage = SqliteStorage::new(db_path).await.unwrap();

        let task_id: TaskId = Ulid::new().into();
        let task = Task {
            id: task_id,
            title: "Original Title".to_string(),
            description: "Original Description".to_string(),
            state: TaskState::Pending,
            allowed_transitions: vec![],
            steps: vec![],
            quality_gate: None,
            snapshots: vec![],
            lightweight_snapshots: vec![],
            metadata: TaskMetadata::default(),
            intent: None,
            verdict: None,
        };

        // Save the task
        storage.save_task(&task).await.unwrap();

        // Update the task
        let mut updated_task = task.clone();
        updated_task.title = "Updated Title".to_string();
        storage.save_task(&updated_task).await.unwrap();

        // Retrieve and verify
        let retrieved = storage.get_task(&task_id).await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Updated Title");
    }
}
