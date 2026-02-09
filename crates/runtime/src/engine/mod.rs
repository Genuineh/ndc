//! Event-Driven Engine - State machine with event listeners
//!
//! Provides event-driven workflow execution with:
//! - State transitions
//! - Event listeners/hooks
//! - Error handling and recovery

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Unique event ID
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventId(pub String);

impl Default for EventId {
    fn default() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Event types in the system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// Task state changed
    TaskStateChanged,
    /// Task started
    TaskStarted,
    /// Task completed
    TaskCompleted,
    /// Task failed
    TaskFailed,
    /// Step started
    StepStarted,
    /// Step completed
    StepCompleted,
    /// Step failed
    StepFailed,
    /// Tool called
    ToolCalled,
    /// Tool succeeded
    ToolSucceeded,
    /// Tool failed
    ToolFailed,
    /// Human intervention needed
    HumanIntervention,
    /// Invariant violated
    InvariantViolated,
    /// Invariant validated
    InvariantValidated,
    /// Discovery phase started
    DiscoveryStarted,
    /// Discovery phase completed
    DiscoveryCompleted,
    /// Quality gate passed
    QualityGatePassed,
    /// Quality gate failed
    QualityGateFailed,
    /// Custom event
    Custom { name: String },
}

/// Event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID
    pub id: EventId,

    /// Event type
    pub event_type: EventType,

    /// Event data
    pub data: EventData,

    /// Source task ID (if applicable)
    pub task_id: Option<String>,

    /// Source step ID (if applicable)
    pub step_id: Option<String>,

    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Event data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventData {
    /// Empty data
    Empty,
    /// Task data
    Task { title: String, state: String },
    /// Step data
    Step { step_number: u32, description: String },
    /// Tool data
    Tool { name: String, args: Vec<String> },
    /// Error data
    Error { message: String, error_type: String },
    /// Invariant data
    Invariant { rule: String, priority: String },
    /// Quality gate data
    QualityGate { gate_name: String, passed: bool },
    /// Custom data
    Custom { key: String, value: String },
}

/// Event handler callback
pub type EventHandler = Box<dyn Fn(&Event) + Send + Sync>;

/// Event listener registration
pub struct EventListener {
    /// Listener ID
    pub id: String,

    /// Event types to listen for
    pub event_types: Vec<EventType>,

    /// Handler callback
    pub handler: EventHandler,

    /// Is enabled?
    pub enabled: bool,
}

impl EventListener {
    /// Create new listener
    pub fn new<F>(id: String, event_types: Vec<EventType>, handler: F) -> Self
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        Self {
            id,
            event_types,
            handler: Box::new(handler),
            enabled: true,
        }
    }
}

/// Event emitter for publishing events
pub struct EventEmitter {
    /// Registered listeners
    listeners: Vec<EventListener>,
}

impl EventEmitter {
    /// Create new emitter
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    /// Register an event listener
    pub fn on<F>(&mut self, id: String, event_types: Vec<EventType>, handler: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        let listener = EventListener::new(id, event_types, handler);
        self.listeners.push(listener);
    }

    /// Emit an event
    pub fn emit(&self, event: &Event) {
        for listener in &self.listeners {
            if !listener.enabled {
                continue;
            }
            if listener.event_types.contains(&event.event_type) {
                (listener.handler)(event);
            }
        }
    }

    /// Get number of listeners
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

/// Workflow state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowState {
    /// Initial state
    Initial,
    /// Planning phase
    Planning,
    /// Discovery phase
    Discovery,
    /// Executing
    Executing,
    /// Verifying
    Verifying,
    /// Completing
    Completing,
    /// Completed
    Completed,
    /// Failed,
    Failed,
    /// Blocked,
    Blocked,
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowState::Initial => write!(f, "Initial"),
            WorkflowState::Planning => write!(f, "Planning"),
            WorkflowState::Discovery => write!(f, "Discovery"),
            WorkflowState::Executing => write!(f, "Executing"),
            WorkflowState::Verifying => write!(f, "Verifying"),
            WorkflowState::Completing => write!(f, "Completing"),
            WorkflowState::Completed => write!(f, "Completed"),
            WorkflowState::Failed => write!(f, "Failed"),
            WorkflowState::Blocked => write!(f, "Blocked"),
        }
    }
}

/// Valid state transitions
const STATE_TRANSITIONS: &[(WorkflowState, WorkflowState)] = &[
    (WorkflowState::Initial, WorkflowState::Planning),
    (WorkflowState::Initial, WorkflowState::Discovery),
    (WorkflowState::Planning, WorkflowState::Discovery),
    (WorkflowState::Planning, WorkflowState::Executing),
    (WorkflowState::Discovery, WorkflowState::Executing),
    (WorkflowState::Executing, WorkflowState::Verifying),
    (WorkflowState::Executing, WorkflowState::Failed),
    (WorkflowState::Executing, WorkflowState::Blocked),
    (WorkflowState::Verifying, WorkflowState::Executing),
    (WorkflowState::Verifying, WorkflowState::Completing),
    (WorkflowState::Verifying, WorkflowState::Failed),
    (WorkflowState::Completing, WorkflowState::Completed),
    (WorkflowState::Failed, WorkflowState::Executing),
    (WorkflowState::Blocked, WorkflowState::Executing),
];

/// Workflow instance
#[derive(Debug, Clone)]
pub struct Workflow {
    /// Workflow ID
    pub id: String,

    /// Current state
    pub state: WorkflowState,

    /// Target state (if transitioning)
    pub target_state: Option<WorkflowState>,

    /// Error message (if failed)
    pub error: Option<String>,

    /// Retry count
    pub retry_count: u32,

    /// Max retries
    pub max_retries: u32,

    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Updated at
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Workflow {
    /// Create new workflow
    pub fn new(id: String) -> Self {
        Self {
            id,
            state: WorkflowState::Initial,
            target_state: None,
            error: None,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Check if transition is valid
    pub fn can_transition(&self, to: WorkflowState) -> bool {
        STATE_TRANSITIONS.contains(&(self.state, to))
    }

    /// Attempt state transition
    pub fn transition(&mut self, to: WorkflowState) -> Result<(), TransitionError> {
        if !self.can_transition(to) {
            return Err(TransitionError::Invalid {
                from: self.state,
                to,
            });
        }

        self.target_state = Some(to);
        self.state = to;
        self.updated_at = chrono::Utc::now();

        Ok(())
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, error: String) {
        self.error = Some(error);
        self.state = WorkflowState::Failed;
        self.updated_at = chrono::Utc::now();
    }

    /// Check if can retry
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        self.updated_at = chrono::Utc::now();
    }
}

/// Transition errors
#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("Invalid transition from {from:?} to {to:?}")]
    Invalid { from: WorkflowState, to: WorkflowState },
}

/// Event-Driven Engine
pub struct EventEngine {
    /// Event emitter
    emitter: EventEmitter,

    /// Active workflows
    workflows: HashMap<String, Workflow>,
}

impl EventEngine {
    /// Create new engine
    pub fn new() -> Self {
        Self {
            emitter: EventEmitter::new(),
            workflows: HashMap::new(),
        }
    }

    /// Create workflow
    pub fn create_workflow(&mut self, workflow_id: String) -> &mut Workflow {
        let id_clone = workflow_id.clone();
        let workflow = Workflow::new(workflow_id);
        self.workflows.insert(id_clone.clone(), workflow);
        self.workflows.get_mut(&id_clone).unwrap()
    }

    /// Get workflow
    pub fn get_workflow(&self, workflow_id: &str) -> Option<&Workflow> {
        self.workflows.get(workflow_id)
    }

    /// Get mutable workflow
    pub fn get_workflow_mut(&mut self, workflow_id: &str) -> Option<&mut Workflow> {
        self.workflows.get_mut(workflow_id)
    }

    /// Transition workflow
    pub fn transition_workflow(
        &mut self,
        workflow_id: &str,
        to: WorkflowState,
    ) -> Result<(), TransitionError> {
        let workflow = self.workflows.get_mut(workflow_id)
            .ok_or(TransitionError::Invalid {
                from: WorkflowState::Initial,
                to,
            })?;

        let from = workflow.state;
        workflow.transition(to)?;

        // Emit state change event
        self.emit_state_change(workflow_id, from, to);

        Ok(())
    }

    /// Emit state change event
    fn emit_state_change(
        &self,
        workflow_id: &str,
        from: WorkflowState,
        to: WorkflowState,
    ) {
        let event = Event {
            id: EventId::default(),
            event_type: EventType::TaskStateChanged,
            data: EventData::Custom {
                key: "state_transition".to_string(),
                value: format!("{:?} -> {:?}", from, to),
            },
            task_id: Some(workflow_id.to_string()),
            step_id: None,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        self.emitter.emit(&event);
    }

    /// Emit an event
    pub fn emit(&self, event: &Event) {
        self.emitter.emit(event);
    }

    /// Register event handler
    pub fn on<F>(&mut self, id: String, event_types: Vec<EventType>, handler: F)
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        self.emitter.on(id, event_types, handler);
    }

    /// Get summary
    pub fn summary(&self) -> EventEngineSummary {
        let mut state_counts: HashMap<WorkflowState, usize> = HashMap::new();

        for workflow in self.workflows.values() {
            *state_counts.entry(workflow.state).or_insert(0) += 1;
        }

        EventEngineSummary {
            total_workflows: self.workflows.len(),
            workflows_by_state: state_counts,
            listener_count: self.emitter.listener_count(),
        }
    }
}

impl Default for EventEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Engine summary
#[derive(Debug, Clone)]
pub struct EventEngineSummary {
    pub total_workflows: usize,
    pub workflows_by_state: HashMap<WorkflowState, usize>,
    pub listener_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_event_engine_new() {
        let engine = EventEngine::new();
        let summary = engine.summary();
        assert_eq!(summary.total_workflows, 0);
        assert_eq!(summary.listener_count, 0);
    }

    #[test]
    fn test_create_workflow() {
        let mut engine = EventEngine::new();
        let workflow = engine.create_workflow("test-1".to_string());

        assert_eq!(workflow.id, "test-1");
        assert_eq!(workflow.state, WorkflowState::Initial);

        let summary = engine.summary();
        assert_eq!(summary.total_workflows, 1);
    }

    #[test]
    fn test_valid_transition() {
        let mut engine = EventEngine::new();
        engine.create_workflow("test-1".to_string());

        let result = engine.transition_workflow("test-1", WorkflowState::Planning);
        assert!(result.is_ok());

        let workflow = engine.get_workflow("test-1").unwrap();
        assert_eq!(workflow.state, WorkflowState::Planning);
    }

    #[test]
    fn test_invalid_transition() {
        let mut engine = EventEngine::new();
        engine.create_workflow("test-1".to_string());

        // Cannot jump from Initial to Executing
        let result = engine.transition_workflow("test-1", WorkflowState::Executing);
        assert!(result.is_err());

        let workflow = engine.get_workflow("test-1").unwrap();
        assert_eq!(workflow.state, WorkflowState::Initial);
    }

    #[test]
    fn test_state_transitions() {
        let mut workflow = Workflow::new("test".to_string());

        assert!(workflow.can_transition(WorkflowState::Planning));
        assert!(!workflow.can_transition(WorkflowState::Executing));

        workflow.transition(WorkflowState::Planning).unwrap();
        assert_eq!(workflow.state, WorkflowState::Planning);

        assert!(workflow.can_transition(WorkflowState::Discovery));
        assert!(workflow.can_transition(WorkflowState::Executing));
    }

    #[test]
    fn test_mark_failed() {
        let mut workflow = Workflow::new("test".to_string());
        workflow.transition(WorkflowState::Planning).unwrap();
        workflow.transition(WorkflowState::Executing).unwrap();

        workflow.mark_failed("Test error".to_string());

        assert_eq!(workflow.state, WorkflowState::Failed);
        assert_eq!(workflow.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_retry() {
        let mut workflow = Workflow::new("test".to_string());
        workflow.max_retries = 2;

        assert!(workflow.can_retry());
        workflow.increment_retry();
        assert_eq!(workflow.retry_count, 1);
        assert!(workflow.can_retry());

        workflow.increment_retry();
        assert_eq!(workflow.retry_count, 2);
        assert!(!workflow.can_retry());
    }

    #[test]
    fn test_workflow_transition_error() {
        let mut workflow = Workflow::new("test".to_string());
        let error = workflow.transition(WorkflowState::Completed);
        assert!(matches!(error, Err(TransitionError::Invalid { .. })));
    }

    #[test]
    fn test_event_emitter() {
        let mut emitter = EventEmitter::new();
        let received: Arc<Mutex<Vec<EventType>>> = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();

        emitter.on(
            "test-listener".to_string(),
            vec![EventType::TaskStarted, EventType::TaskCompleted],
            move |e| {
                received_clone.lock().unwrap().push(e.event_type.clone());
            },
        );

        let started = Event {
            id: EventId::default(),
            event_type: EventType::TaskStarted,
            data: EventData::Empty,
            task_id: None,
            step_id: None,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        let completed = Event {
            id: EventId::default(),
            event_type: EventType::TaskCompleted,
            data: EventData::Empty,
            task_id: None,
            step_id: None,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        emitter.emit(&started);
        emitter.emit(&completed);

        let guard = received.lock().unwrap();
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], EventType::TaskStarted);
        assert_eq!(guard[1], EventType::TaskCompleted);
    }
}
