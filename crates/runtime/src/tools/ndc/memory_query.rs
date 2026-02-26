//! Gold Memory Query Tool - Query persisted invariants/system facts

use async_trait::async_trait;
use ndc_core::{
    InvariantPriority, InvariantQuery, InvariantSourceKind, MemoryContent, MemoryEntry, MemoryId,
};

use super::super::schema::ToolSchemaBuilder;
use super::super::{Tool, ToolError, ToolMetadata, ToolResult};
use ndc_storage::{SharedStorage, create_memory_storage};

#[derive(Clone)]
pub struct MemoryQueryTool {
    storage: SharedStorage,
}

impl MemoryQueryTool {
    pub fn new() -> Self {
        Self::with_storage(create_memory_storage())
    }

    pub fn with_storage(storage: SharedStorage) -> Self {
        Self { storage }
    }

    fn gold_memory_entry_id() -> Result<MemoryId, ToolError> {
        let uuid = uuid::Uuid::parse_str("00000000-0000-0000-0000-00000000a801")
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(MemoryId(uuid))
    }

    fn parse_priority(raw: &str) -> Option<InvariantPriority> {
        match raw.to_ascii_lowercase().as_str() {
            "low" => Some(InvariantPriority::Low),
            "medium" | "normal" => Some(InvariantPriority::Medium),
            "high" => Some(InvariantPriority::High),
            "critical" => Some(InvariantPriority::Critical),
            _ => None,
        }
    }

    fn parse_source_kind(raw: &str) -> Option<InvariantSourceKind> {
        match raw.to_ascii_lowercase().as_str() {
            "human_correction" | "human" => Some(InvariantSourceKind::HumanCorrection),
            "automated_test" | "test" => Some(InvariantSourceKind::AutomatedTest),
            "system_inference" | "system" => Some(InvariantSourceKind::SystemInference),
            "lineage_transfer" | "lineage" => Some(InvariantSourceKind::LineageTransfer),
            _ => None,
        }
    }

    fn parse_tags(raw: Option<&str>) -> Vec<String> {
        raw.unwrap_or_default()
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn decode_service(
        entry: Option<MemoryEntry>,
    ) -> Result<Option<ndc_core::GoldMemoryService>, ToolError> {
        let Some(entry) = entry else {
            return Ok(None);
        };
        match entry.content {
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v2" => {
                let payload: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let service_json = payload.get("service").cloned().ok_or_else(|| {
                    ToolError::ExecutionFailed("invalid gold memory v2 payload".to_string())
                })?;
                let service = serde_json::from_value(service_json)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                Ok(Some(service))
            }
            MemoryContent::General { text, metadata } if metadata == "gold_memory_service/v1" => {
                let service = serde_json::from_str(&text)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                Ok(Some(service))
            }
            _ => Ok(None),
        }
    }
}

impl Default for MemoryQueryTool {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MemoryQueryTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryQueryTool").finish()
    }
}

#[async_trait]
impl Tool for MemoryQueryTool {
    fn name(&self) -> &str {
        "ndc_memory_query"
    }

    fn description(&self) -> &str {
        "Query GoldMemory invariants/system facts by tags, priority, source, and status."
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();
        let priority = params
            .get("priority")
            .and_then(|v| v.as_str())
            .map(|raw| {
                Self::parse_priority(raw).ok_or_else(|| {
                    ToolError::InvalidArgument(format!("Invalid priority filter: {}", raw))
                })
            })
            .transpose()?;
        let source_kind = params
            .get("source")
            .and_then(|v| v.as_str())
            .map(|raw| {
                Self::parse_source_kind(raw).ok_or_else(|| {
                    ToolError::InvalidArgument(format!("Invalid source filter: {}", raw))
                })
            })
            .transpose()?;
        let tags = Self::parse_tags(params.get("tags").and_then(|v| v.as_str()));
        let only_active = params
            .get("only_active")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let min_validation_count = params
            .get("min_validation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(100) as usize;

        let entry_id = Self::gold_memory_entry_id()?;
        let entry = self
            .storage
            .get_memory(&entry_id)
            .await
            .map_err(ToolError::ExecutionFailed)?;
        let Some(service) = Self::decode_service(entry)? else {
            return Ok(ToolResult {
                success: true,
                output: "No GoldMemory facts found.".to_string(),
                error: None,
                metadata: ToolMetadata {
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    files_read: 0,
                    files_written: 0,
                    bytes_processed: 0,
                },
            });
        };

        let query = InvariantQuery {
            priority,
            scope_type: None,
            source_kind,
            tags,
            only_active,
            min_validation_count,
        };
        let mut rows = service.query_invariants(&query);
        rows.sort_by_key(|row| {
            (
                std::cmp::Reverse(row.priority as u8),
                std::cmp::Reverse(row.violation_count),
                row.created_at,
            )
        });

        let mut output = String::from("🧠 GoldMemory Query Results\n\n");
        if rows.is_empty() {
            output.push_str("No matching facts.\n");
        } else {
            for row in rows.into_iter().take(limit) {
                output.push_str(&format!(
                    "- {} [{}] {:?} v={} violations={} tags={}\n",
                    row.id,
                    row.rule,
                    row.priority,
                    row.validation_count,
                    row.violation_count,
                    row.tags.join(",")
                ));
            }
        }

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: ToolMetadata {
                execution_time_ms: start.elapsed().as_millis() as u64,
                files_read: 0,
                files_written: 0,
                bytes_processed: 0,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        ToolSchemaBuilder::new()
            .description("Query GoldMemory facts/invariants")
            .param_string("priority", "Filter by priority: low, medium, high, critical")
            .param_string(
                "source",
                "Filter by source: human_correction, automated_test, system_inference, lineage_transfer",
            )
            .param_string("tags", "Comma-separated tags filter, e.g. verification,quality_gate")
            .param_boolean("only_active", "Only return active facts (default: true)")
            .param_integer("min_validation_count", "Minimum validation count (default: 0)")
            .param_integer("limit", "Maximum rows to return (default: 20, max: 100)")
            .build()
            .to_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_core::{
        AccessControl, AgentId, AgentRole, MemoryMetadata, MemoryStability, SystemFactInput, Task,
    };
    use serde_json::json;

    async fn seed_gold_memory(storage: SharedStorage) {
        let mut service = ndc_core::GoldMemoryService::new();
        service.upsert_system_fact(SystemFactInput {
            dedupe_key: "task:test:quality_gate_failed".to_string(),
            rule: "Quality gate must pass".to_string(),
            description: "quality gate failed".to_string(),
            scope_pattern: "task-test".to_string(),
            priority: InvariantPriority::Critical,
            tags: vec!["verification".to_string(), "quality_gate".to_string()],
            evidence: vec!["kind=quality_gate_failed".to_string()],
            source: "verifier".to_string(),
        });
        service.upsert_system_fact(SystemFactInput {
            dedupe_key: "task:test:discovery_failed".to_string(),
            rule: "Discovery must succeed".to_string(),
            description: "discovery failed".to_string(),
            scope_pattern: "task-test".to_string(),
            priority: InvariantPriority::High,
            tags: vec!["discovery".to_string()],
            evidence: vec!["kind=discovery_failed".to_string()],
            source: "executor_discovery".to_string(),
        });

        let task = Task::new("seed".to_string(), "seed".to_string(), AgentRole::Historian);
        let payload = serde_json::json!({
            "version": 2,
            "service": service
        });
        let entry = MemoryEntry {
            id: MemoryQueryTool::gold_memory_entry_id().unwrap(),
            content: MemoryContent::General {
                text: serde_json::to_string(&payload).unwrap(),
                metadata: "gold_memory_service/v2".to_string(),
            },
            embedding: Vec::new(),
            relations: Vec::new(),
            metadata: MemoryMetadata {
                stability: MemoryStability::Canonical,
                created_at: chrono::Utc::now(),
                created_by: AgentId::system(),
                source_task: task.id,
                version: 2,
                modified_at: Some(chrono::Utc::now()),
                tags: vec!["gold-memory".to_string()],
            },
            access_control: AccessControl::new(AgentId::system(), MemoryStability::Canonical),
        };
        storage.save_memory(&entry).await.unwrap();
    }

    #[tokio::test]
    async fn test_query_empty() {
        let storage = create_memory_storage();
        let tool = MemoryQueryTool::with_storage(storage);
        let result = tool.execute(&json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("No GoldMemory facts found"));
    }

    #[tokio::test]
    async fn test_query_by_priority_and_tag() {
        let storage = create_memory_storage();
        seed_gold_memory(storage.clone()).await;
        let tool = MemoryQueryTool::with_storage(storage);
        let result = tool
            .execute(&json!({
                "priority": "critical",
                "tags": "quality_gate"
            }))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Quality gate must pass"));
        assert!(!result.output.contains("Discovery must succeed"));
    }

    #[tokio::test]
    async fn test_query_by_source() {
        let storage = create_memory_storage();
        seed_gold_memory(storage.clone()).await;
        let tool = MemoryQueryTool::with_storage(storage);
        let result = tool
            .execute(&json!({
                "source": "system_inference"
            }))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("GoldMemory Query Results"));
    }
}
