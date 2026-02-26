//! File Locking - Prevent concurrent editing conflicts
//!
//! Responsibilities:
//! - Advisory file locking for concurrent edit prevention
//! - Lock management with ownership tracking
//! - Lock timeout and automatic release
//! - Integration with edit tool

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;

/// Lock owner identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockOwner {
    /// Owner ID (e.g., session ID, process ID)
    pub id: String,
    /// Owner name for display
    pub name: String,
    /// Timestamp when lock was acquired (not serialized)
    #[doc(hidden)]
    pub acquired_at: Instant,
}

/// Serializable lock owner for persistence
#[derive(Debug, Clone, Serialize)]
struct SerializableLockOwner {
    id: String,
    name: String,
    acquired_at_secs: u64,
}

/// Serializable lock for persistence
#[derive(Debug, Clone, Serialize)]
struct SerializableFileLock {
    path: String,
    owner: SerializableLockOwner,
    expires_at_secs: Option<u64>,
    lock_type: &'static str,
}

/// Type of lock
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LockType {
    /// Read lock - allows concurrent reads but exclusive writes
    Read,
    /// Write lock - exclusive access
    Write,
}

/// File lock information
#[derive(Debug, Clone)]
pub struct FileLock {
    /// Path to the locked file
    pub path: PathBuf,
    /// Owner of the lock
    pub owner: LockOwner,
    /// Lock expiration time (None = no expiration)
    pub expires_at: Option<Instant>,
    /// Lock type
    pub lock_type: LockType,
}

/// Lock error types
#[derive(Debug, Error)]
pub enum LockError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Lock not found for file: {0}")]
    LockNotFound(PathBuf),

    #[error("Lock owned by another process: {0}")]
    LockHeldByOther(String),

    #[error("Lock expired")]
    LockExpired,

    #[error("Lock timeout exceeded")]
    LockTimeout,

    #[error("Invalid lock operation: {0}")]
    InvalidOperation(String),
}

/// Lock operation request
#[derive(Debug, Clone)]
pub struct LockRequest {
    /// Path to lock
    pub path: PathBuf,
    /// Lock type
    pub lock_type: LockType,
    /// Timeout for acquiring lock
    pub timeout_ms: u64,
    /// Whether to fail if lock is held or wait
    pub wait: bool,
}

/// Result of a lock operation
#[derive(Debug, Clone)]
pub struct LockResult {
    /// Whether operation succeeded
    pub success: bool,
    /// The acquired lock (if successful)
    pub lock: Option<FileLock>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Current holder of the lock (if failed due to contention)
    pub current_holder: Option<LockOwner>,
}

/// File Lock Manager
///
/// Manages advisory locks for files to prevent concurrent editing.
/// Uses a combination of in-memory tracking and optional dotfile locking.
#[derive(Debug)]
pub struct FileLockManager {
    /// Lock storage: path -> lock info
    locks: Arc<RwLock<HashMap<PathBuf, FileLock>>>,
    /// Default lock timeout (None = no timeout)
    default_timeout: Option<Duration>,
    /// Lock directory for dotfile storage
    lock_dir: PathBuf,
}

impl Default for FileLockManager {
    fn default() -> Self {
        Self::new(None)
    }
}

impl FileLockManager {
    /// Create a new lock manager
    pub fn new(default_timeout: Option<Duration>) -> Self {
        // Use .ndc/locks in current directory
        let mut lock_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        lock_dir.push(".ndc");
        lock_dir.push("locks");

        // Create lock directory if it doesn't exist
        let _ = std::fs::create_dir_all(&lock_dir);

        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            default_timeout,
            lock_dir,
        }
    }

    /// Create a lock owner
    pub fn create_owner(id: &str, name: &str) -> LockOwner {
        LockOwner {
            id: id.to_string(),
            name: name.to_string(),
            acquired_at: Instant::now(),
        }
    }

    /// Acquire a lock on a file
    pub async fn acquire_lock(
        &self,
        path: &PathBuf,
        owner: &LockOwner,
        lock_type: LockType,
        timeout_ms: u64,
        wait: bool,
    ) -> LockResult {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(timeout_ms);

        loop {
            // Check if we can acquire the lock
            let result = self.try_acquire_lock(path, owner, lock_type).await;

            if result.success {
                return result;
            }

            // Check if we should keep waiting
            if !wait || Instant::now() >= deadline {
                return LockResult {
                    success: false,
                    lock: None,
                    error: Some("Timeout waiting for lock".to_string()),
                    current_holder: result.current_holder,
                };
            }

            // Wait a bit before retrying
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Try to acquire a lock without waiting
    pub async fn try_acquire_lock(
        &self,
        path: &PathBuf,
        owner: &LockOwner,
        lock_type: LockType,
    ) -> LockResult {
        // Normalize path
        let path = self.normalize_path(path);

        // Check if file exists
        if !path.exists() {
            return LockResult {
                success: false,
                lock: None,
                error: Some(format!("File not found: {}", path.display())),
                current_holder: None,
            };
        }

        // Get existing lock
        let mut locks = self.locks.write().await;

        if let Some(existing_lock) = locks.get(&path) {
            // Check if lock has expired
            if let Some(expires_at) = existing_lock.expires_at {
                if Instant::now() > expires_at {
                    // Lock has expired, remove it
                    locks.remove(&path);
                } else {
                    // Lock is still valid, check ownership
                    if existing_lock.owner.id != owner.id {
                        return LockResult {
                            success: false,
                            lock: None,
                            error: Some(format!(
                                "File is locked by {} (ID: {})",
                                existing_lock.owner.name, existing_lock.owner.id
                            )),
                            current_holder: Some(existing_lock.owner.clone()),
                        };
                    }
                    // Same owner - return success (lock refreshed)
                    return LockResult {
                        success: true,
                        lock: Some(existing_lock.clone()),
                        error: None,
                        current_holder: None,
                    };
                }
            } else {
                // Lock never expires, check ownership
                if existing_lock.owner.id != owner.id {
                    return LockResult {
                        success: false,
                        lock: None,
                        error: Some(format!(
                            "File is locked by {} (ID: {})",
                            existing_lock.owner.name, existing_lock.owner.id
                        )),
                        current_holder: Some(existing_lock.owner.clone()),
                    };
                }
                // Same owner - return success (lock refreshed)
                return LockResult {
                    success: true,
                    lock: Some(existing_lock.clone()),
                    error: None,
                    current_holder: None,
                };
            }
        }

        // Calculate expiration
        let expires_at = self.default_timeout.map(|d| Instant::now() + d);

        // Create new lock
        let file_lock = FileLock {
            path: path.clone(),
            owner: owner.clone(),
            expires_at,
            lock_type,
        };

        // Store lock
        locks.insert(path.clone(), file_lock.clone());

        // Also write dotfile for persistence
        self.write_lock_file(&path, &file_lock).await;

        debug!("Lock acquired on {} by {}", path.display(), owner.name);

        LockResult {
            success: true,
            lock: Some(file_lock),
            error: None,
            current_holder: None,
        }
    }

    /// Release a lock
    pub async fn release_lock(&self, path: &PathBuf, owner_id: &str) -> Result<(), LockError> {
        let path = self.normalize_path(path);

        let mut locks = self.locks.write().await;

        if let Some(existing_lock) = locks.get(&path) {
            // Verify ownership
            if existing_lock.owner.id != owner_id {
                return Err(LockError::LockHeldByOther(format!(
                    "Lock held by {} (ID: {}), cannot release",
                    existing_lock.owner.name, existing_lock.owner.id
                )));
            }

            // Remove lock
            locks.remove(&path);
            let _ = self.remove_lock_file(&path).await;

            debug!("Lock released on {} by {}", path.display(), owner_id);

            return Ok(());
        }

        Err(LockError::LockNotFound(path))
    }

    /// Release all locks for an owner
    pub async fn release_all_locks(&self, owner_id: &str) -> usize {
        let mut locks = self.locks.write().await;
        let mut released = 0;

        // Find locks owned by this owner
        let owned_paths: Vec<PathBuf> = locks
            .iter()
            .filter(|(_, lock)| lock.owner.id == owner_id)
            .map(|(path, _)| path.clone())
            .collect();

        // Remove them
        for path in owned_paths {
            locks.remove(&path);
            let _ = self.remove_lock_file(&path).await;
            released += 1;
        }

        debug!("Released {} locks for owner {}", released, owner_id);
        released
    }

    /// Check if a file is locked
    pub async fn is_locked(&self, path: &PathBuf) -> bool {
        let path = self.normalize_path(path);
        let locks = self.locks.read().await;

        if let Some(lock) = locks.get(&path) {
            // Check if lock has expired
            if let Some(expires_at) = lock.expires_at {
                // Lock is valid only if not expired
                Instant::now() <= expires_at
            } else {
                // No expiration = always locked
                true
            }
        } else {
            false
        }
    }

    /// Get lock info for a file
    pub async fn get_lock_info(&self, path: &PathBuf) -> Option<FileLock> {
        let path = self.normalize_path(path);
        let locks = self.locks.read().await;
        locks.get(&path).cloned()
    }

    /// Get all current locks
    pub async fn get_all_locks(&self) -> Vec<FileLock> {
        let locks = self.locks.read().await;
        locks.values().cloned().collect()
    }

    /// Clean up expired locks
    pub async fn cleanup_expired_locks(&self) -> usize {
        let mut locks = self.locks.write().await;
        let now = Instant::now();
        let mut removed = 0;

        // Find expired locks
        let expired_paths: Vec<PathBuf> = locks
            .iter()
            .filter(|(_, lock)| lock.expires_at.is_some_and(|e| now > e))
            .map(|(path, _)| path.clone())
            .collect();

        // Remove expired locks
        for path in expired_paths {
            locks.remove(&path);
            let _ = self.remove_lock_file(&path).await;
            removed += 1;
        }

        if removed > 0 {
            debug!("Cleaned up {} expired locks", removed);
        }

        removed
    }

    /// Normalize path
    fn normalize_path(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    /// Write lock to dotfile
    async fn write_lock_file(&self, path: &Path, lock: &FileLock) {
        let lock_file = self.lock_file_path(path);

        // Create serializable version
        let serializable = SerializableFileLock {
            path: path.to_string_lossy().to_string(),
            owner: SerializableLockOwner {
                id: lock.owner.id.clone(),
                name: lock.owner.name.clone(),
                acquired_at_secs: lock.owner.acquired_at.elapsed().as_secs(),
            },
            expires_at_secs: lock.expires_at.map(|e| e.elapsed().as_secs()),
            lock_type: match lock.lock_type {
                LockType::Read => "read",
                LockType::Write => "write",
            },
        };

        let content = serde_json::to_string_pretty(&serializable).unwrap_or_default();

        let _ = tokio::fs::write(&lock_file, content).await;
    }

    /// Remove lock dotfile
    async fn remove_lock_file(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        let lock_file = self.lock_file_path(path);

        if lock_file.exists() {
            tokio::fs::remove_file(&lock_file).await?;
        }

        Ok(())
    }

    /// Get lock file path
    fn lock_file_path(&self, path: &Path) -> PathBuf {
        // Create a safe filename from the path
        let components = path.components().peekable();
        let mut filename = String::new();

        for comp in components {
            if !filename.is_empty() {
                filename.push('_');
            }
            match comp {
                std::path::Component::Normal(n) => {
                    filename.push_str(&n.to_string_lossy());
                }
                std::path::Component::RootDir => {
                    filename.push('r');
                }
                std::path::Component::ParentDir => {
                    filename.push('p');
                }
                std::path::Component::CurDir => {
                    filename.push('c');
                }
                _ => {}
            }
        }

        // Hash the filename if it's too long
        if filename.len() > 100 {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            filename.hash(&mut hasher);
            let hash = format!("{:x}", hasher.finish());
            filename = format!("lock_{}", hash);
        }

        let mut lock_path = self.lock_dir.clone();
        lock_path.push(&filename);
        lock_path.set_extension("json");

        lock_path
    }
}

/// Edit tool with file locking
#[allow(dead_code)]
#[derive(Debug)]
pub struct EditToolWithLocking {
    /// Lock manager
    lock_manager: Arc<FileLockManager>,
    /// Base edit tool
    edit_tool: super::EditTool,
}

#[allow(dead_code)]
impl EditToolWithLocking {
    /// Create new edit tool with locking
    pub fn new(lock_manager: Arc<FileLockManager>) -> Self {
        Self {
            lock_manager,
            edit_tool: super::EditTool::new(),
        }
    }

    #[allow(dead_code)]
    /// Acquire lock before editing
    async fn acquire_lock_for_edit(
        &self,
        path: &PathBuf,
        owner: &LockOwner,
    ) -> Result<(), LockError> {
        let result = self
            .lock_manager
            .acquire_lock(path, owner, LockType::Write, 30000, true)
            .await;

        if result.success {
            Ok(())
        } else {
            Err(LockError::LockHeldByOther(
                result.error.unwrap_or_else(|| "Unknown".to_string()),
            ))
        }
    }

    /// Release lock after editing
    async fn release_lock_after_edit(
        &self,
        path: &PathBuf,
        owner: &LockOwner,
    ) -> Result<(), LockError> {
        self.lock_manager.release_lock(path, &owner.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(temp_dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let file_path = temp_dir.path().join(name);
        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path
    }

    #[tokio::test]
    async fn test_lock_acquire_and_release() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(&temp_dir, "test.txt", "content");

        let manager = FileLockManager::new(Some(Duration::from_secs(60)));
        let owner = FileLockManager::create_owner("test-id", "Test Owner");

        // Acquire lock
        let result = manager
            .try_acquire_lock(&file_path, &owner, LockType::Write)
            .await;

        assert!(result.success);
        assert!(result.lock.is_some());

        // Check file is locked
        assert!(manager.is_locked(&file_path).await);

        // Try to acquire again (should succeed - same owner)
        let result2 = manager
            .try_acquire_lock(&file_path, &owner, LockType::Write)
            .await;

        assert!(result2.success);

        // Release lock
        manager.release_lock(&file_path, &owner.id).await.unwrap();

        // Check file is not locked
        assert!(!manager.is_locked(&file_path).await);
    }

    #[tokio::test]
    async fn test_lock_contention() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(&temp_dir, "test.txt", "content");

        let manager = FileLockManager::new(Some(Duration::from_secs(60)));
        let owner1 = FileLockManager::create_owner("owner-1", "Owner 1");
        let owner2 = FileLockManager::create_owner("owner-2", "Owner 2");

        // Owner 1 acquires lock
        let result1 = manager
            .try_acquire_lock(&file_path, &owner1, LockType::Write)
            .await;
        assert!(result1.success);

        // Owner 2 tries to acquire (should fail)
        let result2 = manager
            .try_acquire_lock(&file_path, &owner2, LockType::Write)
            .await;

        assert!(!result2.success);
        assert!(result2.current_holder.is_some());
        assert_eq!(result2.current_holder.unwrap().id, "owner-1");
    }

    #[tokio::test]
    async fn test_lock_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_file(&temp_dir, "test.txt", "content");

        let manager = FileLockManager::new(Some(Duration::from_millis(100)));
        let owner1 = FileLockManager::create_owner("owner-1", "Owner 1");
        let owner2 = FileLockManager::create_owner("owner-2", "Owner 2");

        // Owner 1 acquires lock with short timeout
        let result1 = manager
            .try_acquire_lock(&file_path, &owner1, LockType::Write)
            .await;
        assert!(result1.success);

        // Wait for lock to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Owner 2 should be able to acquire
        let result2 = manager
            .try_acquire_lock(&file_path, &owner2, LockType::Write)
            .await;
        assert!(result2.success);
    }

    #[tokio::test]
    async fn test_release_all_locks() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_test_file(&temp_dir, "test1.txt", "content1");
        let file2 = create_test_file(&temp_dir, "test2.txt", "content2");

        let manager = FileLockManager::new(None);
        let owner = FileLockManager::create_owner("owner-1", "Owner 1");

        manager
            .try_acquire_lock(&file1, &owner, LockType::Write)
            .await;
        manager
            .try_acquire_lock(&file2, &owner, LockType::Write)
            .await;

        assert!(manager.is_locked(&file1).await);
        assert!(manager.is_locked(&file2).await);

        let released = manager.release_all_locks(&owner.id).await;
        assert_eq!(released, 2);

        assert!(!manager.is_locked(&file1).await);
        assert!(!manager.is_locked(&file2).await);
    }

    #[tokio::test]
    async fn test_lock_nonexistent_file() {
        let manager = FileLockManager::new(None);
        let owner = FileLockManager::create_owner("owner-1", "Owner 1");

        let result = manager
            .try_acquire_lock(
                &PathBuf::from("/nonexistent/file.txt"),
                &owner,
                LockType::Write,
            )
            .await;

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_cleanup_expired_locks() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_test_file(&temp_dir, "test1.txt", "content1");
        let file2 = create_test_file(&temp_dir, "test2.txt", "content2");

        // Manager with very short timeout
        let manager = FileLockManager::new(Some(Duration::from_millis(50)));
        let owner = FileLockManager::create_owner("owner-1", "Owner 1");

        manager
            .try_acquire_lock(&file1, &owner, LockType::Write)
            .await;
        manager
            .try_acquire_lock(&file2, &owner, LockType::Write)
            .await;

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cleanup
        let cleaned = manager.cleanup_expired_locks().await;
        assert_eq!(cleaned, 2);

        // Both should be unlocked
        assert!(!manager.is_locked(&file1).await);
        assert!(!manager.is_locked(&file2).await);
    }
}
