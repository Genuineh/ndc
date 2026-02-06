//! GitTool - Git 操作工具
//!
//! 提供安全的 Git 操作：
//! - 获取状态
//! - 创建分支
//! - 提交变更
//! - 创建 Worktree（用于快照）

use super::{Tool, ToolResult, ToolError, ToolContext};
use git2::{Repository, BranchType, Signature, Commit};
use std::path::PathBuf;
use std::fs;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, error};

/// Git 工具
#[derive(Debug)]
pub struct GitTool {
    context: ToolContext,
}

impl GitTool {
    pub fn new() -> Self {
        Self {
            context: ToolContext::default(),
        }
    }

    /// 获取 Git 仓库
    fn get_repository(&self) -> Result<Repository, ToolError> {
        Repository::discover(&self.context.working_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

impl Default for GitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Tool for GitTool {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Git operations: status, branch, commit, worktree"
    }

    async fn execute(&self, params: &serde_json::Value) -> Result<ToolResult, ToolError> {
        let operation = params.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgument("Missing 'operation'".to_string()))?;

        let repo = self.get_repository()?;
        let start = std::time::Instant::now();
        let mut output = String::new();

        match operation {
            "status" => {
                let status = repo.statuses(None)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                let mut changes = Vec::new();
                for s in status.iter() {
                    if let Some(path) = s.path() {
                        changes.push(format!("{:?} {}", s.status(), path));
                    }
                }

                output = serde_json::to_string(&changes)
                    .map_err(|e| ToolError::Serialize(e))?;

                debug!("Git status: {} changes", changes.len());
            }

            "branch" => {
                let branch_name = params.get("name")
                    .and_then(|v| v.as_str());

                if let Some(name) = branch_name {
                    // 创建分支
                    let head = repo.head()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let commit = head.peel_to_commit()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                    repo.branch(name, &commit, false)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                    output = format!("Created branch: {}", name);
                    debug!("{}", output);
                } else {
                    // 列出分支
                    let mut branches = Vec::new();

                    repo.branches(None)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
                        .for_each(|b| {
                            if let Ok((branch, _)) = b {
                                let name = branch.name()
                                    .and_then(|n| n.map(|s| s.to_string()))
                                    .unwrap_or_default();
                                branches.push(name);
                            }
                        });

                    output = serde_json::to_string(&branches)
                        .map_err(|e| ToolError::Serialize(e))?;
                    debug!("Listed {} branches", branches.len());
                }
            }

            "commit" => {
                let message = params.get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing 'message'".to_string()))?;

                // 获取索引
                let mut index = repo.index()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                // 添加所有变更
                let patterns: Vec<&str> = vec!["*"];
                index.add_all(patterns, git2::IndexAddOption::DEFAULT, None)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                // 创建 Tree
                let tree_id = index.write_tree()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let tree = repo.find_tree(tree_id)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                // 获取 HEAD
                let head = repo.head()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                let parent = head.peel_to_commit()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                // 创建签名
                let sig = Signature::now("NDC", "ndc@localhost")
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                // 创建提交
                repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                output = format!("Created commit: {}", message);
                debug!("{}", output);
            }

            "worktree" => {
                let worktree_path = params.get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing 'path'".to_string()))?;

                let commitish = params.get("commit")
                    .and_then(|v| v.as_str());

                let target = PathBuf::from(worktree_path);

                // 创建父目录
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                }

                // 创建 worktree
                let commit = if let Some(commit_spec) = commitish {
                    Some(repo.revparse_single(commit_spec)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?)
                } else {
                    None
                };

                repo.worktree(worktree_path, commit.as_ref())
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                output = format!("Created worktree at: {}", worktree_path);
                debug!("{}", output);
            }

            "remove-worktree" => {
                let worktree_path = params.get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArgument("Missing 'path'".to_string()))?;

                // 移除 worktree（只是目录）
                fs::remove_dir_all(worktree_path)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                output = format!("Removed worktree at: {}", worktree_path);
                debug!("{}", output);
            }

            "log" => {
                let max_count = params.get("max_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;

                let mut commits = Vec::new();
                let mut revwalk = repo.revwalk()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                revwalk.push_head()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                for (i, oid) in revwalk.enumerate() {
                    if i >= max_count {
                        break;
                    }

                    let oid = oid
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let commit = repo.find_commit(oid)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                    commits.push(CommitInfo {
                        oid: oid.to_string(),
                        message: commit.message().unwrap_or("").to_string(),
                        time: commit.time().seconds(),
                        author: commit.author().name()
                            .map(|s| s.to_string())
                            .unwrap_or_default(),
                    });
                }

                output = serde_json::to_string(&commits)
                    .map_err(|e| ToolError::Serialize(e))?;
                debug!("Retrieved {} commits", commits.len());
            }

            "diff" => {
                let commit1 = params.get("commit1")
                    .and_then(|v| v.as_str());
                let commit2 = params.get("commit2")
                    .and_then(|v| v.as_str());

                let diff = if let (Some(c1), Some(c2)) = (commit1, commit2) {
                    let c1 = repo.revparse_single(c1)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let c2 = repo.revparse_single(c2)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                    let c1 = c1.peel_to_commit()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let c2 = c2.peel_to_commit()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                    let t1 = c1.tree()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                    let t2 = c2.tree()
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?

                } else {
                    // 工作目录与 HEAD 比较
                    repo.diff_index_to_workdir(None)
                        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
                };

                let mut diff_output = String::new();
                diff.print(git2::DiffFormat::Patch, |_, _, _| true)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

                output = diff_output;
                debug!("Generated diff");
            }

            _ => {
                return Err(ToolError::InvalidArgument(
                    format!("Unknown operation: {}", operation)
                ));
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            metadata: super::ToolMetadata {
                execution_time_ms: duration,
                files_read: 0,
                files_written: 0,
                bytes_processed: output.len() as u64,
            },
        })
    }

    fn schema(&self) -> serde_json::Value {
        serde::json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["status", "branch", "commit", "worktree", "remove-worktree", "log", "diff"],
                    "description": "Git operation"
                },
                "path": { "type": "string", "description": "Path for worktree" },
                "message": { "type": "string", "description": "Commit message" },
                "name": { "type": "string", "description": "Branch name" },
                "commit": { "type": "string", "description": "Commit reference" },
                "max_count": { "type": "number", "description": "Max commits to return" }
            },
            "required": ["operation"]
        })
    }
}

/// 提交信息
#[derive(Debug, Serialize, Deserialize)]
pub struct CommitInfo {
    pub oid: String,
    pub message: String,
    pub time: i64,
    pub author: String,
}
