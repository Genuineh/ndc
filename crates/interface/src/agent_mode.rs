//! Agent Mode - AI Agent REPL Integration
//!
//! 职责:
//! - REPL 的 Agent 交互模式
//! - /agent 命令处理
//! - 流式响应显示
//! - 权限确认 UI
//!
//! 设计理念 (来自 NDC_AGENT_INTEGRATION_PLAN.md):
//! - 使用 OpenCode 的流式响应模式
//! - 使用 OpenCode 的权限确认模式
//! - 增强内置 NDC 工程能力
//! - 集成 NDC 反馈循环验证

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast, mpsc};
use tracing::{debug, info};

use ndc_core::{
    AbstractHistory, AgentConfig, AgentError, AgentOrchestrator, AgentRequest, AgentResponse,
    AgentRole, ApiSurface, FailurePattern, InvariantPriority, LlmProvider, ModelInfo,
    NdcConfigLoader, ProviderType, RawCurrent, StepContext, SubTaskId, TaskId, TaskStorage,
    TaskVerifier, TrajectoryState, VersionedInvariant, WorkingMemory,
};
use ndc_runtime::{Executor, SharedStorage, tools::ToolRegistry};

use crate::provider_config::{create_provider_config, is_minimax_family};

use crate::project_index::{
    ProjectIndexStore, build_project_scoped_session_id, canonicalize_existing_dir,
    discover_project_directories,
};

use crate::session_archive::SessionArchiveStore;

/// Runtime storage adapter - 给 TaskVerifier 使用同一份任务存储
struct RuntimeTaskStorage {
    storage: SharedStorage,
}

impl RuntimeTaskStorage {
    fn new(storage: SharedStorage) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl TaskStorage for RuntimeTaskStorage {
    async fn get_task(
        &self,
        id: &TaskId,
    ) -> Result<Option<ndc_core::Task>, Box<dyn std::error::Error + Send + Sync>> {
        self.storage.get_task(id).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> =
                Box::new(std::io::Error::other(e));
            boxed
        })
    }

    async fn save_memory(
        &self,
        memory: &ndc_core::MemoryEntry,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.storage.save_memory(memory).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> =
                Box::new(std::io::Error::other(e));
            boxed
        })
    }

    async fn get_memory(
        &self,
        id: &ndc_core::MemoryId,
    ) -> Result<Option<ndc_core::MemoryEntry>, Box<dyn std::error::Error + Send + Sync>> {
        self.storage.get_memory(id).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> =
                Box::new(std::io::Error::other(e));
            boxed
        })
    }
}

/// Agent REPL 模式配置
#[derive(Debug, Clone)]
pub struct AgentModeConfig {
    /// Agent 名称
    pub agent_name: String,

    /// Agent 描述
    pub description: String,

    /// LLM Provider 名称
    pub provider: String,

    /// 模型名称
    pub model: String,

    /// 温度
    pub temperature: f32,

    /// 最大工具调用次数
    pub max_tool_calls: usize,

    /// 是否启用流式响应
    pub enable_streaming: bool,

    /// 是否自动验证
    pub auto_verify: bool,

    /// 权限规则: 操作 -> allow/ask/deny
    pub permissions: HashMap<String, PermissionRule>,
}

pub use crate::permission_engine::PermissionRule;

impl Default for AgentModeConfig {
    fn default() -> Self {
        let mut permissions = HashMap::new();
        // 默认权限规则：未知操作需确认
        permissions.insert("*".to_string(), PermissionRule::Ask);
        // 安全的只读操作直接放行
        permissions.insert("file_read".to_string(), PermissionRule::Allow);
        permissions.insert("task_manage".to_string(), PermissionRule::Allow);
        // 危险操作需要用户确认
        permissions.insert("file_write".to_string(), PermissionRule::Ask);
        permissions.insert("file_delete".to_string(), PermissionRule::Ask);
        permissions.insert("git_commit".to_string(), PermissionRule::Ask);
        permissions.insert("shell_execute".to_string(), PermissionRule::Ask);
        permissions.insert("network".to_string(), PermissionRule::Ask);

        let mut config = Self {
            agent_name: "build".to_string(),
            description: "NDC default agent with engineering capabilities".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.1,
            max_tool_calls: 50,
            enable_streaming: true,
            auto_verify: true,
            permissions,
        };

        // Prefer configured provider/model when available.
        let mut loader = NdcConfigLoader::new();
        if loader.load().is_ok()
            && let Some(llm) = loader.config().llm.as_ref()
        {
            config.provider = llm.provider.clone();
            config.model = llm.model.clone();
        }

        config
    }
}

/// Agent REPL 模式状态
#[derive(Debug, Clone, Default)]
pub struct AgentModeState {
    /// 是否启用
    pub enabled: bool,

    /// 当前配置
    pub config: AgentModeConfig,

    /// Agent 会话 ID
    pub session_id: Option<String>,

    /// 活跃任务 ID
    pub active_task_id: Option<TaskId>,

    /// 工作目录
    pub working_dir: Option<PathBuf>,

    /// 当前项目 ID
    pub project_id: Option<String>,

    /// 当前项目根目录
    pub project_root: Option<PathBuf>,

    /// 当前工作树根目录
    pub worktree: Option<PathBuf>,
}

/// Agent REPL 模式管理器
pub struct AgentModeManager {
    /// 状态
    state: Arc<Mutex<AgentModeState>>,

    /// Orchestrator (可选，仅当启用时创建)
    orchestrator: Arc<Mutex<Option<AgentOrchestrator>>>,

    /// Runtime Executor (保留供未来使用)
    _executor: Arc<Executor>,

    /// Tool Registry
    tool_registry: Arc<ToolRegistry>,

    /// Runtime working directory shared with tool executor.
    runtime_working_dir: Arc<Mutex<Option<PathBuf>>>,

    /// Persistent cross-process project index.
    project_index: Arc<Mutex<ProjectIndexStore>>,

    /// Persistent cross-process session archive.
    session_archive: Arc<Mutex<SessionArchiveStore>>,

    /// Channel sender for TUI permission prompts.
    permission_tx: Arc<Mutex<Option<mpsc::Sender<PermissionRequest>>>>,
}

impl AgentModeManager {
    /// 创建新的 Agent Mode Manager
    pub fn new(executor: Arc<Executor>, tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            state: Arc::new(Mutex::new(AgentModeState::default())),
            orchestrator: Arc::new(Mutex::new(None)),
            _executor: executor,
            tool_registry,
            runtime_working_dir: Arc::new(Mutex::new(None)),
            project_index: Arc::new(Mutex::new(ProjectIndexStore::load_default())),
            session_archive: Arc::new(Mutex::new(SessionArchiveStore::load_default())),
            permission_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the TUI permission channel for interactive confirmation.
    pub async fn set_permission_channel(&self, tx: mpsc::Sender<PermissionRequest>) {
        *self.permission_tx.lock().await = Some(tx);
    }

    /// 启用 Agent 模式
    pub async fn enable(&self, config: AgentModeConfig) -> Result<(), AgentError> {
        let detected_identity =
            ndc_core::ProjectIdentity::detect(Some(self._executor.context().project_root.clone()));
        let bootstrap_session_id =
            build_project_scoped_session_id(detected_identity.project_id.as_str());

        // 创建 Agent Orchestrator
        let mut executor = ReplToolExecutor::new(
            self.tool_registry.clone(),
            config.permissions.clone(),
            self.runtime_working_dir.clone(),
        );
        if let Some(tx) = self.permission_tx.lock().await.clone() {
            executor = executor.with_permission_channel(tx);
        }
        let tool_executor = Arc::new(executor);
        let provider = self.create_provider(&config.provider, &config.model)?;

        // TaskVerifier 与工具调用共享同一份 runtime storage
        let storage = Arc::new(RuntimeTaskStorage::new(
            self._executor.context().storage.clone(),
        ));
        let verifier = Arc::new(TaskVerifier::new(storage).with_gold_memory(Arc::new(
            std::sync::Mutex::new(ndc_core::GoldMemoryService::new()),
        )));

        let agent_config = AgentConfig {
            max_tool_calls: config.max_tool_calls,
            enable_streaming: config.enable_streaming,
            auto_verify: config.auto_verify,
            ..Default::default()
        };

        let orchestrator = AgentOrchestrator::new(provider, tool_executor, verifier, agent_config);
        self.hydrate_orchestrator_sessions(&orchestrator).await;

        let restored_session_id = orchestrator
            .latest_session_id_for_project(detected_identity.project_id.as_str())
            .await;
        let active_session_id = restored_session_id.unwrap_or(bootstrap_session_id);
        let active_identity = orchestrator
            .session_project_identity(active_session_id.as_str())
            .await
            .unwrap_or(detected_identity);

        {
            let mut state = self.state.lock().await;
            state.enabled = true;
            state.config = config.clone();
            state.session_id = Some(active_session_id.clone());
            state.working_dir = Some(active_identity.working_dir.clone());
            state.project_id = Some(active_identity.project_id.clone());
            state.project_root = Some(active_identity.project_root.clone());
            state.worktree = Some(active_identity.worktree.clone());
        }
        self.sync_runtime_project_context(&active_identity).await;
        self.remember_project_identity(&active_identity, Some(active_session_id.as_str()))
            .await;

        let mut orch = self.orchestrator.lock().await;
        *orch = Some(orchestrator);

        info!(agent = %config.agent_name, "Agent mode enabled");
        Ok(())
    }

    /// 禁用 Agent 模式
    pub async fn disable(&self) {
        let mut state = self.state.lock().await;
        state.enabled = false;
        state.session_id = None;
        state.active_task_id = None;
        state.working_dir = None;
        state.project_id = None;
        state.project_root = None;
        state.worktree = None;

        let mut orch = self.orchestrator.lock().await;
        *orch = None;

        let mut runtime_working_dir = self.runtime_working_dir.lock().await;
        *runtime_working_dir = None;

        info!("Agent mode disabled");
    }

    /// 检查是否启用
    pub async fn is_enabled(&self) -> bool {
        let state = self.state.lock().await;
        state.enabled
    }

    /// 处理用户输入 (非流式)
    pub async fn process_input(&self, input: &str) -> Result<AgentResponse, AgentError> {
        let state = self.state.lock().await;

        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }

        let session_id = state.session_id.clone();
        let working_dir = state.working_dir.clone();
        let active_task_id = state.active_task_id;

        drop(state);

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };

        let request = AgentRequest {
            user_input: input.to_string(),
            session_id,
            working_dir,
            role: Some(AgentRole::Implementer),
            active_task_id,
            working_memory: self.build_working_memory(active_task_id).await,
        };

        let response = orchestrator.process(request).await?;
        let identity = {
            let state = self.state.lock().await;
            let Some(project_id) = state.project_id.clone() else {
                return Ok(response);
            };
            let Some(project_root) = state.project_root.clone() else {
                return Ok(response);
            };
            let Some(working_dir) = state.working_dir.clone() else {
                return Ok(response);
            };
            let Some(worktree) = state.worktree.clone() else {
                return Ok(response);
            };
            ndc_core::ProjectIdentity {
                project_id,
                project_root,
                working_dir,
                worktree,
            }
        };
        self.remember_project_identity(&identity, Some(response.session_id.as_str()))
            .await;
        self.persist_session_snapshot(response.session_id.as_str())
            .await;
        Ok(response)
    }

    /// Start a new project-scoped session within current project context.
    pub async fn start_new_session(&self) -> Result<String, AgentError> {
        let identity_hint = {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            state
                .working_dir
                .clone()
                .or_else(|| state.project_root.clone())
        };
        let identity = ndc_core::ProjectIdentity::detect(identity_hint);
        let next_session_id = build_project_scoped_session_id(identity.project_id.as_str());

        let mut state = self.state.lock().await;
        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }
        state.session_id = Some(next_session_id.clone());
        state.working_dir = Some(identity.working_dir.clone());
        state.project_id = Some(identity.project_id.clone());
        state.project_root = Some(identity.project_root.clone());
        state.worktree = Some(identity.worktree.clone());
        drop(state);
        self.sync_runtime_project_context(&identity).await;
        self.remember_project_identity(&identity, Some(next_session_id.as_str()))
            .await;
        Ok(next_session_id)
    }

    /// Resume latest session in current project.
    pub async fn resume_latest_project_session(&self) -> Result<String, AgentError> {
        let (current_project_id, fallback_hint) = {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            (
                state.project_id.clone(),
                state
                    .working_dir
                    .clone()
                    .or_else(|| state.project_root.clone()),
            )
        };
        let current_identity = ndc_core::ProjectIdentity::detect(fallback_hint);
        let project_id = current_project_id.unwrap_or_else(|| current_identity.project_id.clone());

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };

        let session_id = orchestrator
            .latest_session_id_for_project(project_id.as_str())
            .await
            .ok_or_else(|| {
                AgentError::SessionNotFound(format!(
                    "No session found for project '{}'",
                    project_id
                ))
            })?;

        if let Some(identity) = orchestrator
            .session_project_identity(session_id.as_str())
            .await
        {
            let identity_for_runtime = identity.clone();
            let mut state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            state.session_id = Some(session_id.clone());
            state.project_id = Some(identity.project_id);
            state.project_root = Some(identity.project_root);
            state.working_dir = Some(identity.working_dir);
            state.worktree = Some(identity.worktree);
            drop(state);
            self.sync_runtime_project_context(&identity_for_runtime)
                .await;
            self.remember_project_identity(&identity_for_runtime, Some(session_id.as_str()))
                .await;
        }

        Ok(session_id)
    }

    /// Use a specific session id, with optional explicit cross-project override.
    pub async fn use_session(
        &self,
        session_id: &str,
        allow_cross_project_session: bool,
    ) -> Result<String, AgentError> {
        let (current_project_id, fallback_hint) = {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            (
                state.project_id.clone(),
                state
                    .working_dir
                    .clone()
                    .or_else(|| state.project_root.clone()),
            )
        };
        let current_identity = ndc_core::ProjectIdentity::detect(fallback_hint);
        let active_project_id =
            current_project_id.unwrap_or_else(|| current_identity.project_id.clone());

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        let target_identity = orchestrator
            .session_project_identity(session_id)
            .await
            .ok_or_else(|| {
                AgentError::SessionNotFound(format!("Session '{}' not found", session_id))
            })?;

        let cross_project = target_identity.project_id != active_project_id;
        if cross_project && !allow_cross_project_session {
            return Err(AgentError::InvalidRequest(format!(
                "session '{}' belongs to project '{}', current project is '{}'; pass --allow-cross-project-session to override",
                session_id, target_identity.project_id, active_project_id
            )));
        }

        let mut state = self.state.lock().await;
        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }
        state.session_id = Some(session_id.to_string());
        state.project_id = Some(target_identity.project_id.clone());
        state.project_root = Some(target_identity.project_root.clone());
        state.working_dir = Some(target_identity.working_dir.clone());
        state.worktree = Some(target_identity.worktree.clone());
        drop(state);
        self.sync_runtime_project_context(&target_identity).await;
        self.remember_project_identity(&target_identity, Some(session_id))
            .await;

        Ok(session_id.to_string())
    }

    /// Discover nearby projects for interactive switching.
    pub async fn discover_projects(
        &self,
        limit: usize,
    ) -> Result<Vec<AgentProjectCandidate>, AgentError> {
        let (current_working_dir, current_project_root) = {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            (state.working_dir.clone(), state.project_root.clone())
        };
        let current_identity_hint = current_project_root
            .clone()
            .or_else(|| current_working_dir.clone());

        let mut seed_dirs = self.persisted_project_roots(limit.saturating_mul(2)).await;
        if let Some(path) = current_working_dir {
            seed_dirs.push(path);
        }
        if let Some(path) = current_project_root {
            seed_dirs.push(path);
        }
        if let Ok(cwd) = std::env::current_dir() {
            seed_dirs.push(cwd);
        }
        if seed_dirs.is_empty() {
            seed_dirs.push(PathBuf::from("."));
        }

        let roots = discover_project_directories(seed_dirs.as_slice(), limit.saturating_mul(4));
        let mut seen = BTreeSet::<String>::new();
        let mut candidates = Vec::<AgentProjectCandidate>::new();
        if let Some(path) = current_identity_hint {
            let identity = ndc_core::ProjectIdentity::detect(Some(path));
            let key = format!(
                "{}::{}",
                identity.project_id,
                identity.project_root.display()
            );
            if seen.insert(key) {
                candidates.push(AgentProjectCandidate {
                    project_id: identity.project_id,
                    project_root: identity.project_root,
                    working_dir: identity.working_dir,
                    worktree: identity.worktree,
                });
            }
        }
        for root in roots {
            let identity = ndc_core::ProjectIdentity::detect(Some(root));
            let key = format!(
                "{}::{}",
                identity.project_id,
                identity.project_root.display()
            );
            if !seen.insert(key) {
                continue;
            }
            candidates.push(AgentProjectCandidate {
                project_id: identity.project_id,
                project_root: identity.project_root,
                working_dir: identity.working_dir,
                worktree: identity.worktree,
            });
            if candidates.len() >= limit.max(1) {
                break;
            }
        }
        Ok(candidates)
    }

    /// Switch to a project context using a directory.
    pub async fn switch_project_context(
        &self,
        directory: PathBuf,
    ) -> Result<ProjectSwitchOutcome, AgentError> {
        {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
        }

        let directory = if directory.is_absolute() {
            directory
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(directory)
        };
        let directory = canonicalize_existing_dir(directory.as_path()).ok_or_else(|| {
            AgentError::InvalidRequest(format!(
                "project directory not found or not a directory: {}",
                directory.display()
            ))
        })?;
        let identity = ndc_core::ProjectIdentity::detect(Some(directory));

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        let latest_session = orchestrator
            .latest_session_id_for_project(identity.project_id.as_str())
            .await;
        let session_id = latest_session
            .clone()
            .unwrap_or_else(|| build_project_scoped_session_id(identity.project_id.as_str()));

        let mut state = self.state.lock().await;
        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }
        state.active_task_id = None;
        state.session_id = Some(session_id.clone());
        state.project_id = Some(identity.project_id.clone());
        state.project_root = Some(identity.project_root.clone());
        state.working_dir = Some(identity.working_dir.clone());
        state.worktree = Some(identity.worktree.clone());
        drop(state);
        self.sync_runtime_project_context(&identity).await;
        self.remember_project_identity(&identity, Some(session_id.as_str()))
            .await;

        Ok(ProjectSwitchOutcome {
            project_id: identity.project_id,
            project_root: identity.project_root,
            working_dir: identity.working_dir,
            worktree: identity.worktree,
            session_id,
            resumed_existing_session: latest_session.is_some(),
        })
    }

    /// Return known project ids from in-memory session index.
    pub async fn known_project_ids(&self) -> Result<Vec<String>, AgentError> {
        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        let mut ids = orchestrator
            .known_project_ids()
            .await
            .into_iter()
            .collect::<BTreeSet<_>>();
        let persisted_ids = {
            let store = self.project_index.lock().await;
            store.known_project_ids()
        };
        ids.extend(persisted_ids);
        Ok(ids.into_iter().collect())
    }

    /// Return recent session ids for a project (defaults to current project).
    pub async fn list_project_session_ids(
        &self,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, AgentError> {
        let fallback_project_id = {
            let state = self.state.lock().await;
            if !state.enabled {
                return Err(AgentError::InvalidRequest(
                    "Agent mode is not enabled".to_string(),
                ));
            }
            state.project_id.clone()
        };
        let project_id = project_id
            .map(|value| value.to_string())
            .or(fallback_project_id)
            .ok_or_else(|| AgentError::InvalidRequest("No current project".to_string()))?;

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        Ok(orchestrator
            .session_ids_for_project(project_id.as_str(), Some(limit.max(1)))
            .await)
    }

    async fn persisted_project_roots(&self, limit: usize) -> Vec<PathBuf> {
        let store = self.project_index.lock().await;
        store.known_project_roots(limit)
    }

    async fn hydrate_orchestrator_sessions(&self, orchestrator: &AgentOrchestrator) {
        let mut sessions = {
            let store = self.session_archive.lock().await;
            store.all_sessions()
        };
        if sessions.is_empty() {
            return;
        }
        // Archive is stored newest-first by last_seen; hydrate oldest->newest so
        // project latest-session cursor lands on the most recently active session.
        sessions.reverse();
        orchestrator.hydrate_sessions(sessions).await;
    }

    async fn persist_session_snapshot(&self, session_id: &str) {
        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned()
        };
        let Some(orchestrator) = orchestrator else {
            return;
        };
        let Some(session) = orchestrator.session_snapshot(session_id).await else {
            return;
        };
        let mut store = self.session_archive.lock().await;
        store.upsert(&session);
        if let Err(err) = store.save() {
            debug!(
                error = %err,
                path = %store.path.display(),
                "failed to persist session archive"
            );
        }
    }

    async fn remember_project_identity(
        &self,
        identity: &ndc_core::ProjectIdentity,
        session_id: Option<&str>,
    ) {
        let mut store = self.project_index.lock().await;
        store.upsert(identity, session_id);
        if let Err(err) = store.save() {
            debug!(
                error = %err,
                path = %store.path.display(),
                "failed to persist project index"
            );
        }
    }

    async fn sync_runtime_project_context(&self, identity: &ndc_core::ProjectIdentity) {
        let mut working_dir = self.runtime_working_dir.lock().await;
        *working_dir = Some(identity.working_dir.clone());
        drop(working_dir);
        Self::apply_runtime_project_environment(identity);
    }

    fn apply_runtime_project_environment(identity: &ndc_core::ProjectIdentity) {
        if cfg!(test) {
            return;
        }
        unsafe {
            std::env::set_var("NDC_PROJECT_ROOT", &identity.project_root);
        }
        if let Err(err) = std::env::set_current_dir(&identity.working_dir) {
            debug!(
                error = %err,
                working_dir = %identity.working_dir.display(),
                "failed to update process current directory for project switch"
            );
        }
    }

    /// 获取当前会话的执行时间线（用于 REPL /timeline 重放）
    pub async fn session_timeline(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<ndc_core::AgentExecutionEvent>, AgentError> {
        let state = self.state.lock().await;
        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }
        let session_id = state
            .session_id
            .clone()
            .ok_or_else(|| AgentError::InvalidRequest("No active session id".to_string()))?;
        drop(state);

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        match orchestrator
            .get_session_execution_events(&session_id, limit)
            .await
        {
            Ok(events) => Ok(events),
            Err(AgentError::SessionNotFound(_)) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// 订阅实时执行事件（会话级）
    pub async fn subscribe_execution_events(
        &self,
    ) -> Result<
        (
            String,
            broadcast::Receiver<ndc_core::AgentSessionExecutionEvent>,
        ),
        AgentError,
    > {
        let state = self.state.lock().await;
        if !state.enabled {
            return Err(AgentError::InvalidRequest(
                "Agent mode is not enabled".to_string(),
            ));
        }
        let session_id = state
            .session_id
            .clone()
            .ok_or_else(|| AgentError::InvalidRequest("No active session id".to_string()))?;
        drop(state);

        let orchestrator = {
            let orch = self.orchestrator.lock().await;
            orch.as_ref().cloned().ok_or_else(|| {
                AgentError::InvalidRequest("Orchestrator not initialized".to_string())
            })?
        };
        Ok((session_id, orchestrator.subscribe_execution_events()))
    }

    async fn build_working_memory(&self, active_task_id: Option<TaskId>) -> Option<WorkingMemory> {
        let task_id = active_task_id?;
        let storage = self._executor.context().storage.clone();
        let task = storage.get_task(&task_id).await.ok().flatten()?;

        let mut active_files = Self::collect_task_files(&task);
        active_files.sort();
        active_files.dedup();

        let failure_patterns = Self::collect_failure_patterns(&task);
        let attempt_count = failure_patterns.len().min(u8::MAX as usize) as u8;
        let trajectory_state = if attempt_count == 0 {
            TrajectoryState::Progressing {
                steps_since_last_failure: task.steps.len().min(u8::MAX as usize) as u8,
            }
        } else if attempt_count >= 3 {
            TrajectoryState::Cycling {
                repeated_pattern: "repeated task-step failures".to_string(),
            }
        } else {
            let last = failure_patterns
                .last()
                .map(|f| f.message.clone())
                .unwrap_or_else(|| "unknown".to_string());
            TrajectoryState::Stuck { last_error: last }
        };

        let hard_invariants = self.collect_task_hard_invariants(&task).await;
        let abstract_history = AbstractHistory {
            failure_patterns,
            root_cause_summary: Self::compose_root_cause_summary(attempt_count, &hard_invariants),
            attempt_count,
            trajectory_state,
        };

        let step_context = task.steps.last().map(|step| StepContext {
            description: format!("{:?}", step.action),
            step_index: step.step_id as u32,
            expected_output: step.result.as_ref().map(|r| r.output.clone()),
        });

        let raw_current = RawCurrent {
            active_files: active_files.into_iter().map(PathBuf::from).collect(),
            api_surface: Vec::<ApiSurface>::new(),
            current_step_context: step_context,
        };

        Some(WorkingMemory::generate(
            SubTaskId(task_id.to_string()),
            Some(abstract_history),
            raw_current,
            hard_invariants,
        ))
    }

    fn compose_root_cause_summary(
        attempt_count: u8,
        hard_invariants: &[VersionedInvariant],
    ) -> Option<String> {
        if attempt_count == 0 && hard_invariants.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if attempt_count > 0 {
            parts.push("Derived from task execution failures".to_string());
        }
        if !hard_invariants.is_empty() {
            let rules = hard_invariants
                .iter()
                .map(|inv| inv.rule.clone())
                .collect::<Vec<_>>();
            parts.push(format!("Constraints: {}", rules.join(" | ")));
        }
        Some(parts.join("; "))
    }

    fn collect_task_files(task: &ndc_core::Task) -> Vec<String> {
        let mut files = Vec::new();

        if let Some(intent) = task.intent.as_ref() {
            Self::extract_action_path_strings(&intent.proposed_action, &mut files);
        }
        for step in &task.steps {
            Self::extract_action_path_strings(&step.action, &mut files);
        }

        files
    }

    fn extract_action_path_strings(action: &ndc_core::Action, out: &mut Vec<String>) {
        match action {
            ndc_core::Action::ReadFile { path }
            | ndc_core::Action::WriteFile { path, .. }
            | ndc_core::Action::CreateFile { path }
            | ndc_core::Action::DeleteFile { path } => {
                out.push(path.display().to_string());
            }
            _ => {}
        }
    }

    fn collect_failure_patterns(task: &ndc_core::Task) -> Vec<FailurePattern> {
        task.steps
            .iter()
            .filter_map(|step| {
                if step.status == ndc_core::StepStatus::Failed
                    || step.result.as_ref().map(|r| !r.success).unwrap_or(false)
                {
                    Some(FailurePattern {
                        error_type: "task_step_failure".to_string(),
                        message: step
                            .result
                            .as_ref()
                            .and_then(|r| r.error.clone())
                            .unwrap_or_else(|| format!("step {} failed", step.step_id)),
                        file: None,
                        line: None,
                        timestamp: chrono::Utc::now(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    async fn collect_task_hard_invariants(&self, task: &ndc_core::Task) -> Vec<VersionedInvariant> {
        let mut invariants = Vec::new();

        if let Some(gate) = task.quality_gate.as_ref() {
            for (idx, check) in gate.checks.iter().enumerate() {
                invariants.push(VersionedInvariant {
                    id: format!("qg-{}-{}", task.id, idx),
                    rule: format!("Quality gate must pass: {:?}", check.check_type),
                    scope: task.id.to_string(),
                    priority: InvariantPriority::High,
                    created_at: chrono::Utc::now(),
                    ttl_days: None,
                    version_tags: Vec::new(),
                });
            }
        }

        let storage = self._executor.context().storage.clone();
        for step in &task.steps {
            if let Some(result) = step.result.as_ref() {
                for mem_id in &result.metrics.memory_access {
                    if let Ok(Some(memory)) = storage.get_memory(mem_id).await {
                        let rule = match memory.content {
                            ndc_core::MemoryContent::ErrorSolution(ref e) => {
                                format!("Avoid known failure: {}", e.prevention)
                            }
                            ndc_core::MemoryContent::Decision(ref d) => {
                                format!("Follow recorded decision: {}", d.decision)
                            }
                            ndc_core::MemoryContent::General { ref text, .. } => {
                                format!("General memory constraint: {}", text)
                            }
                            _ => continue,
                        };
                        invariants.push(VersionedInvariant {
                            id: format!("mem-{}", mem_id.0),
                            rule,
                            scope: task.id.to_string(),
                            priority: InvariantPriority::Medium,
                            created_at: chrono::Utc::now(),
                            ttl_days: None,
                            version_tags: Vec::new(),
                        });
                    }
                }
            }
        }

        invariants
    }

    /// 设置活跃任务
    pub async fn set_active_task(&self, task_id: TaskId) {
        let mut state = self.state.lock().await;
        state.active_task_id = Some(task_id);
    }

    /// 获取状态信息
    pub async fn status(&self) -> AgentModeStatus {
        let state = self.state.lock().await;
        AgentModeStatus {
            enabled: state.enabled,
            agent_name: state.config.agent_name.clone(),
            provider: state.config.provider.clone(),
            model: state.config.model.clone(),
            session_id: state.session_id.clone(),
            active_task_id: state.active_task_id,
            project_id: state.project_id.clone(),
            project_root: state.project_root.clone(),
            worktree: state.worktree.clone(),
        }
    }

    fn pick_preferred_minimax_model(models: &[ModelInfo]) -> Option<String> {
        let available: std::collections::HashSet<String> =
            models.iter().map(|m| m.id.to_ascii_lowercase()).collect();
        for preferred in ["minimax-m2.5", "minimax-m2"] {
            if available.contains(preferred) {
                if preferred == "minimax-m2.5" {
                    return Some("MiniMax-M2.5".to_string());
                }
                return Some("MiniMax-M2".to_string());
            }
        }
        models.first().map(|m| m.id.clone())
    }

    async fn resolve_default_model(&self, provider_name: &str) -> String {
        match provider_name {
            "openai" => "gpt-4o".to_string(),
            "anthropic" => "claude-sonnet-4-5-20250929".to_string(),
            "openrouter" => "anthropic/claude-3.5-sonnet".to_string(),
            "ollama" => "llama3.2".to_string(),
            p if is_minimax_family(p) => {
                let bootstrap_model = "MiniMax-M2.5";
                let provider = match self.create_provider(provider_name, bootstrap_model) {
                    Ok(p) => p,
                    Err(_) => return bootstrap_model.to_string(),
                };
                match provider.list_models().await {
                    Ok(models) if !models.is_empty() => Self::pick_preferred_minimax_model(&models)
                        .unwrap_or_else(|| bootstrap_model.to_string()),
                    _ => bootstrap_model.to_string(),
                }
            }
            _ => provider_name.to_string(),
        }
    }

    fn provider_bootstrap_model(provider_name: &str) -> &'static str {
        match provider_name {
            "openai" => "gpt-4o",
            "anthropic" => "claude-sonnet-4-5-20250929",
            "openrouter" => "anthropic/claude-3.5-sonnet",
            "ollama" => "llama3.2",
            p if is_minimax_family(p) => "MiniMax-M2.5",
            _ => "gpt-4o",
        }
    }

    /// List models for a provider (or current provider if omitted).
    pub async fn list_models(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<ModelInfo>, AgentError> {
        let (provider_name_owned, current_model) = {
            let state = self.state.lock().await;
            (
                provider_name
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| state.config.provider.clone()),
                state.config.model.clone(),
            )
        };

        let bootstrap = if current_model.is_empty() {
            Self::provider_bootstrap_model(&provider_name_owned).to_string()
        } else {
            current_model
        };
        let provider = self.create_provider(&provider_name_owned, &bootstrap)?;
        provider
            .list_models()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))
    }

    /// Switch model while keeping current provider.
    pub async fn switch_model(&self, model: &str) -> Result<(), AgentError> {
        let provider = {
            let state = self.state.lock().await;
            state.config.provider.clone()
        };
        self.switch_provider(&provider, Some(model)).await
    }

    /// 切换 LLM Provider
    pub async fn switch_provider(
        &self,
        provider_name: &str,
        model: Option<&str>,
    ) -> Result<(), AgentError> {
        let mut state = self.state.lock().await;

        // 检查是否启用
        let was_enabled = state.enabled;

        // 更新配置
        state.config.provider = provider_name.to_string();
        let new_model = if let Some(m) = model {
            m.to_string()
        } else {
            self.resolve_default_model(provider_name).await
        };
        state.config.model = new_model.clone();

        // 克隆更新后的配置
        let config = state.config.clone();

        drop(state);

        // 重新创建 orchestrator (如果之前已启用)
        if was_enabled {
            self.disable().await;
            self.enable(config).await?;
        }

        info!(provider = %provider_name, model = %new_model, "Provider switched");
        Ok(())
    }

    /// 创建 LLM Provider
    fn create_provider(
        &self,
        provider_name: &str,
        model: &str,
    ) -> Result<Arc<dyn LlmProvider>, AgentError> {
        use ndc_core::llm::provider::{
            AnthropicProvider, OpenAiProvider, OpenRouterProvider, SimpleTokenCounter, TokenCounter,
        };

        // 根据 provider 名称创建相应的 Provider
        let provider_type: ProviderType = if is_minimax_family(provider_name) {
            ProviderType::MiniMax
        } else {
            provider_name.to_string().into()
        };
        let token_counter: Arc<dyn TokenCounter> = Arc::new(SimpleTokenCounter::new());

        match provider_type {
            ProviderType::OpenAi => {
                let config = create_provider_config(provider_name, model);
                let provider = OpenAiProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::Anthropic => {
                let config = create_provider_config(provider_name, model);
                let provider = AnthropicProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::MiniMax => {
                let config = create_provider_config(provider_name, model);
                // Align with OpenCode: use Anthropic-compatible MiniMax endpoint.
                let provider = AnthropicProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::OpenRouter => {
                let config = create_provider_config(provider_name, model);
                let provider = OpenRouterProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            ProviderType::Ollama => {
                let config = create_provider_config(provider_name, model);
                let provider = OpenAiProvider::new(config, token_counter);
                Ok(Arc::new(provider))
            }
            _ => Err(AgentError::InvalidRequest(format!(
                "Provider '{}' is not supported. Supported: openai, anthropic, minimax, openrouter, ollama",
                provider_name
            ))),
        }
    }
}

/// Agent 模式状态信息
#[derive(Debug, Clone)]
pub struct AgentModeStatus {
    pub enabled: bool,
    pub agent_name: String,
    pub provider: String,
    pub model: String,
    pub session_id: Option<String>,
    pub active_task_id: Option<TaskId>,
    pub project_id: Option<String>,
    pub project_root: Option<PathBuf>,
    pub worktree: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct AgentProjectCandidate {
    pub project_id: String,
    pub project_root: PathBuf,
    pub working_dir: PathBuf,
    pub worktree: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProjectSwitchOutcome {
    pub project_id: String,
    pub project_root: PathBuf,
    pub working_dir: PathBuf,
    pub worktree: PathBuf,
    pub session_id: String,
    pub resumed_existing_session: bool,
}

// PermissionRequest and ReplToolExecutor are defined in permission_engine module
pub use crate::permission_engine::{PermissionRequest, ReplToolExecutor};

/// 显示 Agent 状态
pub fn show_agent_status(status: AgentModeStatus) {
    println!("\n┌─────────────────────────────────────────────────────────────────┐");
    println!("│  AI Agent Mode Status                                            │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!(
        "│  Status: {}                                                     │",
        if status.enabled {
            "🟢 Enabled"
        } else {
            "⚪ Disabled"
        }
    );
    if status.enabled {
        println!(
            "│  Agent: {}                                                      │",
            status.agent_name
        );
        println!(
            "│  Provider: {} @ {}                                               │",
            status.provider, status.model
        );
        if let Some(sid) = &status.session_id {
            println!(
                "│  Session: {}                                                   │",
                sid
            );
        }
        if let Some(pid) = &status.project_id {
            println!(
                "│  Project: {}                                                   │",
                pid
            );
        }
        if let Some(tid) = &status.active_task_id {
            println!(
                "│  Active Task: {}                                                │",
                tid
            );
        }
    }
    println!("└─────────────────────────────────────────────────────────────────┘\n");
}

/// 处理 /agent 命令
pub async fn handle_agent_command(
    input: &str,
    manager: &AgentModeManager,
) -> Result<bool, AgentError> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts.get(1).unwrap_or(&"help");

    match *cmd {
        "on" | "enable" => {
            let config = AgentModeConfig::default();
            manager.enable(config).await?;
            println!("\n✅ Agent Mode Enabled\n");
            show_agent_status(manager.status().await);
            println!("💡 Type your message to interact with the AI agent.");
            println!("   Use '/agent off' to disable.\n");
            Ok(true)
        }
        "off" | "disable" => {
            manager.disable().await;
            println!("\n🔴 Agent Mode Disabled\n");
            Ok(true)
        }
        "status" => {
            show_agent_status(manager.status().await);
            Ok(true)
        }
        "help" => {
            show_agent_help();
            Ok(true)
        }
        _ => {
            println!("Unknown agent command: {}", cmd);
            show_agent_help();
            Ok(true)
        }
    }
}

/// 显示 Agent 命令帮助
fn show_agent_help() {
    println!("\n┌─────────────────────────────────────────────────────────────────┐");
    println!("│  Agent Mode Commands                                             │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  /agent on       Enable AI agent mode                            │");
    println!("│  /agent off      Disable AI agent mode                           │");
    println!("│  /agent status   Show agent status                               │");
    println!("│  /agent help     Show this help message                          │");
    println!("├─────────────────────────────────────────────────────────────────┤");
    println!("│  When agent mode is enabled:                                      │");
    println!("│  - Your messages will be processed by the AI agent               │");
    println!("│  - The agent can use tools to complete tasks                     │");
    println!("│  - Use /agent off to return to normal REPL mode                  │");
    println!("└─────────────────────────────────────────────────────────────────┘\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{
        Action, AgentExecutionEvent, AgentExecutionEventKind, AgentRole, GateStrategy,
        QualityCheck, QualityCheckType, QualityGate, Task,
    };
    use ndc_runtime::{ExecutionContext, Executor, create_default_tool_registry_with_storage};
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    #[test]
    fn test_agent_mode_config_default() {
        let config = AgentModeConfig::default();
        assert_eq!(config.agent_name, "build");
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4o");
        assert!(config.enable_streaming);
        assert!(config.auto_verify);
    }

    #[test]
    fn test_permission_rule() {
        let allow = PermissionRule::Allow;
        let ask = PermissionRule::Ask;
        let deny = PermissionRule::Deny;

        assert_eq!(allow, PermissionRule::Allow);
        assert_eq!(ask, PermissionRule::Ask);
        assert_eq!(deny, PermissionRule::Deny);
    }

    #[test]
    fn test_agent_mode_state_default() {
        let state = AgentModeState::default();
        assert!(!state.enabled);
        assert_eq!(state.config.agent_name, "build");
        assert!(state.session_id.is_none());
        assert!(state.active_task_id.is_none());
    }

    #[tokio::test]
    async fn test_agent_mode_manager_create() {
        // This is a basic smoke test - full integration tests require
        // more setup with actual Executor and ToolRegistry
        let config = AgentModeConfig::default();
        assert_eq!(config.agent_name, "build");
    }

    #[tokio::test]
    async fn test_session_timeline_empty_before_first_message() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();
        let timeline = manager.session_timeline(Some(20)).await.unwrap();
        assert!(timeline.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_execution_events_returns_receiver() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();
        let (session_id, mut rx) = manager.subscribe_execution_events().await.unwrap();
        assert!(!session_id.is_empty());
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn test_start_new_session_rotates_session_id() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();
        let before = manager.status().await.session_id.unwrap_or_default();
        assert!(!before.is_empty());

        let next = manager.start_new_session().await.unwrap();
        let after = manager.status().await.session_id.unwrap_or_default();
        assert_eq!(next, after);
        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn test_resume_latest_project_session_without_history_returns_not_found() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();
        let result = manager.resume_latest_project_session().await;
        assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_use_session_unknown_returns_not_found() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();
        let result = manager.use_session("missing-session", false).await;
        assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_switch_project_context_updates_status_and_discovery() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);

        manager.enable(AgentModeConfig::default()).await.unwrap();

        let temp = TempDir::new().expect("temp dir");
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .expect("write marker");
        let expected_root = std::fs::canonicalize(temp.path()).expect("canonical");

        let outcome = manager
            .switch_project_context(temp.path().to_path_buf())
            .await
            .expect("switch project");
        assert_eq!(outcome.project_root, expected_root);

        let status = manager.status().await;
        assert_eq!(status.project_root.as_ref(), Some(&expected_root));
        assert_eq!(status.session_id, Some(outcome.session_id.clone()));

        let discovered = manager.discover_projects(20).await.expect("discover");
        assert!(
            discovered
                .iter()
                .any(|candidate| candidate.project_root == expected_root)
        );

        let sessions = manager
            .list_project_session_ids(None, 10)
            .await
            .expect("list sessions");
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_discover_projects_includes_persisted_index_entries() {
        let _guard = env_lock();
        let temp = TempDir::new().expect("temp dir");
        let index_path = temp.path().join("project_index.json");
        unsafe {
            std::env::set_var(
                "NDC_PROJECT_INDEX_FILE",
                index_path.to_string_lossy().to_string(),
            );
        }

        let project_a = temp.path().join("project-a");
        let project_b = temp.path().join("project-b");
        std::fs::create_dir_all(project_a.as_path()).expect("create project a");
        std::fs::create_dir_all(project_b.as_path()).expect("create project b");
        std::fs::write(
            project_a.join("Cargo.toml"),
            "[package]\nname=\"project-a\"\nversion=\"0.1.0\"\n",
        )
        .expect("write marker a");
        std::fs::write(project_b.join("package.json"), "{\"name\":\"project-b\"}")
            .expect("write marker b");

        let identity_a = ndc_core::ProjectIdentity::detect(Some(project_a));
        let identity_b = ndc_core::ProjectIdentity::detect(Some(project_b));
        {
            let mut store = ProjectIndexStore::load_default();
            store.upsert(&identity_a, None);
            store.upsert(&identity_b, None);
            store.save().expect("save seeded index");
        }

        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage));
        let manager = AgentModeManager::new(executor, tool_registry);
        let known_roots = {
            let store = manager.project_index.lock().await;
            store.known_project_roots(20)
        };
        assert!(
            known_roots.contains(&identity_a.project_root),
            "persisted store missing identity_a: {:?}",
            known_roots
        );
        assert!(
            known_roots.contains(&identity_b.project_root),
            "persisted store missing identity_b: {:?}",
            known_roots
        );
        manager.enable(AgentModeConfig::default()).await.unwrap();

        let discovered = manager.discover_projects(20).await.expect("discover");
        let discovered_roots = discovered
            .iter()
            .map(|candidate| candidate.project_root.display().to_string())
            .collect::<Vec<_>>();
        assert!(
            discovered
                .iter()
                .any(|candidate| candidate.project_root == identity_a.project_root),
            "missing project-a root: {} in {:?}",
            identity_a.project_root.display(),
            discovered_roots
        );
        assert!(
            discovered
                .iter()
                .any(|candidate| candidate.project_root == identity_b.project_root),
            "missing project-b root: {} in {:?}",
            identity_b.project_root.display(),
            discovered_roots
        );

        let known_ids = manager.known_project_ids().await.expect("known projects");
        assert!(known_ids.contains(&identity_a.project_id));
        assert!(known_ids.contains(&identity_b.project_id));

        unsafe {
            std::env::remove_var("NDC_PROJECT_INDEX_FILE");
        }
    }

    #[tokio::test]
    async fn test_enable_restores_session_from_archive_and_timeline() {
        let _guard = env_lock();
        let temp = TempDir::new().expect("temp dir");
        let archive_path = temp.path().join("session_archive.json");
        let index_path = temp.path().join("project_index.json");
        unsafe {
            std::env::set_var(
                "NDC_SESSION_ARCHIVE_FILE",
                archive_path.to_string_lossy().to_string(),
            );
        }
        unsafe {
            std::env::set_var(
                "NDC_PROJECT_INDEX_FILE",
                index_path.to_string_lossy().to_string(),
            );
        }

        let project_root = temp.path().join("project");
        std::fs::create_dir_all(project_root.as_path()).expect("create project");
        std::fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname=\"project\"\nversion=\"0.1.0\"\n",
        )
        .expect("write marker");
        let identity = ndc_core::ProjectIdentity::detect(Some(project_root.clone()));

        let mut context_a = ExecutionContext::default();
        context_a.project_root = project_root.clone();
        let storage_a = context_a.storage.clone();
        let executor_a = Arc::new(Executor::new(context_a));
        let tool_registry_a = Arc::new(create_default_tool_registry_with_storage(storage_a));
        let manager_a = AgentModeManager::new(executor_a, tool_registry_a);
        manager_a.enable(AgentModeConfig::default()).await.unwrap();

        let archived_session_id = "agent-restored-session";
        let mut archived_session = ndc_core::AgentSession::new_with_project_identity(
            archived_session_id.to_string(),
            identity,
        );
        archived_session.add_execution_event(AgentExecutionEvent {
            kind: AgentExecutionEventKind::WorkflowStage,
            timestamp: chrono::Utc::now(),
            message: "workflow_stage: planning | restore".to_string(),
            round: 1,
            tool_name: None,
            tool_call_id: None,
            duration_ms: None,
            is_error: false,
            workflow_stage: Some(ndc_core::AgentWorkflowStage::Planning),
            workflow_detail: Some("restore".to_string()),
            workflow_stage_index: Some(1),
            workflow_stage_total: Some(ndc_core::AgentWorkflowStage::TOTAL_STAGES),
        });

        {
            let orch = manager_a.orchestrator.lock().await;
            let orchestrator = orch.as_ref().cloned().expect("orchestrator");
            drop(orch);
            orchestrator
                .upsert_session_snapshot(archived_session.clone())
                .await;
        }
        manager_a
            .persist_session_snapshot(archived_session_id)
            .await;

        let mut context_b = ExecutionContext::default();
        context_b.project_root = project_root.clone();
        let storage_b = context_b.storage.clone();
        let executor_b = Arc::new(Executor::new(context_b));
        let tool_registry_b = Arc::new(create_default_tool_registry_with_storage(storage_b));
        let manager_b = AgentModeManager::new(executor_b, tool_registry_b);
        manager_b.enable(AgentModeConfig::default()).await.unwrap();

        let status = manager_b.status().await;
        assert_eq!(status.session_id.as_deref(), Some(archived_session_id));
        let timeline = manager_b
            .session_timeline(Some(10))
            .await
            .expect("timeline");
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].message, "workflow_stage: planning | restore");

        unsafe {
            std::env::remove_var("NDC_PROJECT_INDEX_FILE");
        }
        unsafe {
            std::env::remove_var("NDC_SESSION_ARCHIVE_FILE");
        }
    }

    #[tokio::test]
    async fn test_build_working_memory_from_active_task() {
        let context = ExecutionContext::default();
        let storage = context.storage.clone();
        let executor = Arc::new(Executor::new(context));
        let tool_registry = Arc::new(create_default_tool_registry_with_storage(storage.clone()));
        let manager = AgentModeManager::new(executor, tool_registry);

        let mut task = Task::new(
            "WM Task".to_string(),
            "working memory source".to_string(),
            AgentRole::Implementer,
        );
        task.quality_gate = Some(QualityGate {
            checks: vec![QualityCheck {
                check_type: QualityCheckType::Test,
                command: None,
                pass_condition: ndc_core::PassCondition::ExitCode(0),
            }],
            strategy: GateStrategy::AllMustPass,
        });
        task.steps.push(ndc_core::ExecutionStep {
            step_id: 1,
            action: Action::ReadFile {
                path: PathBuf::from("/tmp/test.txt"),
            },
            status: ndc_core::StepStatus::Failed,
            result: Some(ndc_core::ActionResult {
                success: false,
                output: String::new(),
                error: Some("file read failed".to_string()),
                metrics: ndc_core::ActionMetrics::default(),
            }),
            executed_at: Some(chrono::Utc::now()),
        });
        storage.save_task(&task).await.unwrap();

        let wm = manager.build_working_memory(Some(task.id)).await;
        assert!(wm.is_some());
        let wm = wm.unwrap();
        assert!(!wm.raw_current.active_files.is_empty());
        assert!(!wm.abstract_history.failure_patterns.is_empty());
        assert!(!wm.hard_invariants.is_empty());
        assert!(
            wm.abstract_history
                .root_cause_summary
                .as_ref()
                .map(|s| s.contains("Quality gate must pass"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_pick_preferred_minimax_model() {
        let models = vec![
            ModelInfo {
                id: "foo-model".to_string(),
                object: "model".to_string(),
                created: 0,
                owned_by: "minimax".to_string(),
                permission: vec![],
            },
            ModelInfo {
                id: "MiniMax-M2".to_string(),
                object: "model".to_string(),
                created: 0,
                owned_by: "minimax".to_string(),
                permission: vec![],
            },
        ];
        let picked = AgentModeManager::pick_preferred_minimax_model(&models);
        assert_eq!(picked.as_deref(), Some("MiniMax-M2"));
    }

    #[test]
    fn test_default_wildcard_permission_is_ask() {
        let config = AgentModeConfig::default();
        assert_eq!(
            config.permissions.get("*"),
            Some(&PermissionRule::Ask),
            "Wildcard default must be Ask, not Allow"
        );
    }

    #[test]
    fn test_read_only_operations_are_allowed_by_default() {
        let config = AgentModeConfig::default();
        assert_eq!(
            config.permissions.get("file_read"),
            Some(&PermissionRule::Allow),
            "file_read should be explicitly Allow"
        );
        assert_eq!(
            config.permissions.get("task_manage"),
            Some(&PermissionRule::Allow),
            "task_manage should be explicitly Allow"
        );
    }

    #[test]
    fn test_dangerous_operations_require_ask() {
        let config = AgentModeConfig::default();
        assert_eq!(
            config.permissions.get("shell_execute"),
            Some(&PermissionRule::Ask)
        );
        assert_eq!(
            config.permissions.get("network"),
            Some(&PermissionRule::Ask)
        );
        assert_eq!(
            config.permissions.get("file_write"),
            Some(&PermissionRule::Ask)
        );
        assert_eq!(
            config.permissions.get("file_delete"),
            Some(&PermissionRule::Ask)
        );
        assert_eq!(
            config.permissions.get("git_commit"),
            Some(&PermissionRule::Ask)
        );
    }
}
