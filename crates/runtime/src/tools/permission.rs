//! Permission System - Tool permission control
//!
//! Responsibilities:
//! - Dangerous operation detection
//! - Permission confirmation requests
//! - Permission usage logging
//!
//! Design参考 OpenCode permission.ts

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::{debug, warn};

/// 权限错误
#[derive(Debug, Error)]
pub enum PermissionError {
    #[error("Permission denied: {0}")]
    Denied(String),

    #[error("Operation requires confirmation: {0}")]
    RequiresConfirmation(String),

    #[error("Invalid permission scope: {0}")]
    InvalidScope(String),
}

/// 危险操作级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerLevel {
    /// 安全操作
    Safe = 0,
    /// 低风险
    Low = 1,
    /// 中等风险
    Medium = 2,
    /// 高风险
    High = 3,
    /// 极高风险
    Critical = 4,
}

impl DangerLevel {
    /// 比较危险级别是否小于等于
    pub fn le(&self, other: &DangerLevel) -> bool {
        (*self as u8) <= (*other as u8)
    }

    /// 比较危险级别是否大于等于
    pub fn ge(&self, other: &DangerLevel) -> bool {
        (*self as u8) >= (*other as u8)
    }

    /// 检查是否是极高风险
    pub fn is_critical(&self) -> bool {
        *self == DangerLevel::Critical
    }
}

/// 权限类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionType {
    /// 读取文件
    Read,
    /// 写入文件
    Write,
    /// 删除文件
    Delete,
    /// 执行命令
    Execute,
    /// 网络访问
    Network,
    /// Git 操作
    Git,
}

/// 权限请求
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// 操作类型
    pub permission_type: PermissionType,
    /// 目标路径
    pub path: Option<PathBuf>,
    /// 操作描述
    pub description: String,
    /// 危险级别
    pub danger_level: DangerLevel,
    /// 是否已确认
    pub confirmed: bool,
}

/// 权限响应
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionResponse {
    Allow,
    Deny,
    Confirm(String),  // 确认码
}

/// 权限配置
#[derive(Debug, Clone)]
pub struct PermissionConfig {
    /// 是否启用权限检查
    pub enabled: bool,

    /// 自动允许的危险级别
    pub auto_allow_level: DangerLevel,

    /// 需要确认的危险级别
    pub require_confirm_level: DangerLevel,

    /// 危险命令模式
    pub dangerous_patterns: Vec<DangerousPattern>,

    /// 权限缓存时间（秒）
    pub cache_ttl_seconds: u64,
}

/// 危险命令模式
#[derive(Debug, Clone)]
pub struct DangerousPattern {
    /// 命令或模式
    pub pattern: String,
    /// 危险级别
    pub level: DangerLevel,
    /// 描述
    pub description: String,
}

/// 权限缓存条目
#[derive(Debug, Clone)]
struct CacheEntry {
    /// 请求哈希
    hash: String,
    /// 响应
    response: PermissionResponse,
    /// 创建时间
    created_at: SystemTime,
}

/// 权限系统
#[derive(Debug, Clone)]
pub struct PermissionSystem {
    /// 配置
    config: PermissionConfig,

    /// 权限缓存
    cache: Vec<CacheEntry>,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_allow_level: DangerLevel::Low,
            require_confirm_level: DangerLevel::High,
            cache_ttl_seconds: 300,  // 5 minutes
            dangerous_patterns: Self::default_dangerous_patterns(),
        }
    }
}

impl PermissionConfig {
    /// 默认危险命令模式
    fn default_dangerous_patterns() -> Vec<DangerousPattern> {
        vec![
            // Critical - 极高风险
            DangerousPattern {
                pattern: "rm -rf /".to_string(),
                level: DangerLevel::Critical,
                description: "Delete root directory".to_string(),
            },
            DangerousPattern {
                pattern: "rm -rf /usr".to_string(),
                level: DangerLevel::Critical,
                description: "Delete system directory".to_string(),
            },
            DangerousPattern {
                pattern: "mkfs".to_string(),
                level: DangerLevel::Critical,
                description: "Format filesystem".to_string(),
            },

            // High - 高风险
            DangerousPattern {
                pattern: "rm -rf".to_string(),
                level: DangerLevel::High,
                description: "Recursive delete".to_string(),
            },
            DangerousPattern {
                pattern: "chmod -R 777".to_string(),
                level: DangerLevel::High,
                description: "World writable permissions".to_string(),
            },
            DangerousPattern {
                pattern: "chown -R".to_string(),
                level: DangerLevel::High,
                description: "Recursive ownership change".to_string(),
            },
            DangerousPattern {
                pattern: "> /dev/sda".to_string(),
                level: DangerLevel::High,
                description: "Write to disk device".to_string(),
            },
            DangerousPattern {
                pattern: "dd if=/dev/zero".to_string(),
                level: DangerLevel::High,
                description: "Disk wipe".to_string(),
            },

            // Medium - 中等风险
            DangerousPattern {
                pattern: "kill -9".to_string(),
                level: DangerLevel::Medium,
                description: "Force kill process".to_string(),
            },
            DangerousPattern {
                pattern: "pkill".to_string(),
                level: DangerLevel::Medium,
                description: "Kill processes by name".to_string(),
            },
            DangerousPattern {
                pattern: "killall".to_string(),
                level: DangerLevel::Medium,
                description: "Kill all processes".to_string(),
            },
            DangerousPattern {
                pattern: "reboot".to_string(),
                level: DangerLevel::Medium,
                description: "Reboot system".to_string(),
            },
            DangerousPattern {
                pattern: "shutdown".to_string(),
                level: DangerLevel::Medium,
                description: "Shutdown system".to_string(),
            },

            // Low - 低风险（但仍需注意）
            DangerousPattern {
                pattern: "curl ".to_string(),
                level: DangerLevel::Low,
                description: "Network request".to_string(),
            },
            DangerousPattern {
                pattern: "wget ".to_string(),
                level: DangerLevel::Low,
                description: "Download file".to_string(),
            },
        ]
    }
}

impl PermissionSystem {
    /// 创建权限系统
    pub fn new(config: Option<PermissionConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            cache: Vec::new(),
        }
    }

    /// 检查权限
    pub async fn check(&mut self, request: PermissionRequest) -> Result<PermissionResponse, PermissionError> {
        // 如果权限检查禁用，允许所有操作
        if !self.config.enabled {
            return Ok(PermissionResponse::Allow);
        }

        // 检查危险级别
        let level = self.assess_danger(&request);

        // 如果危险级别在自动允许范围内，直接允许
        if self.should_auto_allow(level) {
            debug!("Permission auto-allowed: {:?}", request.description);
            return Ok(PermissionResponse::Allow);
        }

        // 生成请求哈希
        let hash = self.hash_request(&request);

        // 检查缓存
        if let Some(cached) = self.get_cached(&hash) {
            debug!("Permission cached: {:?}", request.description);
            return Ok(cached);
        }

        // 需要确认
        if self.should_confirm(level) {
            let response = PermissionResponse::Confirm(hash.clone());
            self.cache_response(hash, response.clone());
            return Err(PermissionError::RequiresConfirmation(
                format!("Operation requires confirmation: {}", request.description)
            ));
        }

        // 默认拒绝高风险操作
        if level.ge(&DangerLevel::High) {
            warn!("Permission denied: {:?}", request.description);
            return Err(PermissionError::Denied(
                format!("High-risk operation denied: {}", request.description)
            ));
        }

        // 默认允许低风险操作
        Ok(PermissionResponse::Allow)
    }

    /// 确认权限请求
    pub async fn confirm(&mut self, hash: &str, confirm: bool) -> Result<PermissionResponse, PermissionError> {
        if confirm {
            // 更新缓存
            for entry in &mut self.cache {
                if entry.hash == hash {
                    entry.response = PermissionResponse::Allow;
                    return Ok(PermissionResponse::Allow);
                }
            }
            Err(PermissionError::InvalidScope(hash.to_string()))
        } else {
            Err(PermissionError::Denied("User denied operation".to_string()))
        }
    }

    /// 评估危险级别
    fn assess_danger(&self, request: &PermissionRequest) -> DangerLevel {
        // 如果已经有明确的危险级别，使用它
        if request.danger_level != DangerLevel::Safe {
            return request.danger_level;
        }

        // 根据操作类型评估
        match request.permission_type {
            PermissionType::Read => DangerLevel::Safe,
            PermissionType::Write => DangerLevel::Low,
            PermissionType::Delete => DangerLevel::Medium,
            PermissionType::Execute => DangerLevel::Medium,
            PermissionType::Network => DangerLevel::Low,
            PermissionType::Git => DangerLevel::Low,
        }
    }

    /// 检查是否应自动允许
    fn should_auto_allow(&self, level: DangerLevel) -> bool {
        level.le(&self.config.auto_allow_level)
    }

    /// 检查是否需要确认
    fn should_confirm(&self, level: DangerLevel) -> bool {
        level.ge(&self.config.require_confirm_level) && !level.is_critical()
    }

    /// 检查命令是否危险
    pub fn check_command(&self, command: &str, args: &[String]) -> Option<DangerLevel> {
        let full_command = if args.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, args.join(" "))
        };

        for pattern in &self.config.dangerous_patterns {
            if full_command.contains(&pattern.pattern) {
                return Some(pattern.level);
            }
        }

        None
    }

    /// 生成请求哈希
    fn hash_request(&self, request: &PermissionRequest) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();

        let input = format!(
            "{:?}:{:?}:{}",
            request.permission_type,
            request.path,
            request.description
        );

        hasher.update(input);
        format!("{:x}", hasher.finalize())
    }

    /// 获取缓存的响应
    fn get_cached(&self, hash: &str) -> Option<PermissionResponse> {
        for entry in &self.cache {
            if entry.hash == hash {
                let elapsed = entry.created_at.elapsed().unwrap_or(Duration::MAX);
                if elapsed < Duration::from_secs(self.config.cache_ttl_seconds) {
                    return Some(entry.response.clone());
                }
            }
        }
        None
    }

    /// 缓存响应
    fn cache_response(&mut self, hash: String, response: PermissionResponse) {
        self.cache.push(CacheEntry {
            hash,
            response,
            created_at: SystemTime::now(),
        });

        // 清理过期缓存
        self.cleanup_cache();
    }

    /// 清理过期缓存
    fn cleanup_cache(&mut self) {
        let ttl = Duration::from_secs(self.config.cache_ttl_seconds);
        self.cache.retain(|entry| {
            entry.created_at.elapsed().map(|e| e < ttl).unwrap_or(true)
        });
    }

    /// 获取配置
    pub fn config(&self) -> &PermissionConfig {
        &self.config
    }

    /// 获取配置（可变）
    pub fn config_mut(&mut self) -> &mut PermissionConfig {
        &mut self.config
    }
}

/// 权限系统构建器
#[derive(Debug, Default)]
pub struct PermissionSystemBuilder {
    config: PermissionConfig,
}

impl PermissionSystemBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置是否启用权限检查
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// 设置自动允许的危险级别
    pub fn auto_allow_level(mut self, level: DangerLevel) -> Self {
        self.config.auto_allow_level = level;
        self
    }

    /// 设置需要确认的危险级别
    pub fn require_confirm_level(mut self, level: DangerLevel) -> Self {
        self.config.require_confirm_level = level;
        self
    }

    /// 添加危险命令模式
    pub fn add_dangerous_pattern(mut self, pattern: DangerousPattern) -> Self {
        self.config.dangerous_patterns.push(pattern);
        self
    }

    /// 构建权限系统
    pub fn build(self) -> PermissionSystem {
        PermissionSystem::new(Some(self.config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_config_default() {
        let config = PermissionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.auto_allow_level, DangerLevel::Low);
        assert_eq!(config.require_confirm_level, DangerLevel::High);
        assert!(config.dangerous_patterns.len() > 0);
    }

    #[test]
    fn test_check_command_critical() {
        let system = PermissionSystem::new(None);
        assert_eq!(
            system.check_command("rm", &["-rf".to_string(), "/".to_string()]),
            Some(DangerLevel::Critical)
        );
    }

    #[test]
    fn test_check_command_high() {
        let system = PermissionSystem::new(None);
        // "chmod -R 777" 匹配 High 级别模式
        assert_eq!(
            system.check_command("chmod", &["-R".to_string(), "777".to_string()]),
            Some(DangerLevel::High)
        );
    }

    #[test]
    fn test_check_command_safe() {
        let system = PermissionSystem::new(None);
        assert_eq!(
            system.check_command("echo", &["hello".to_string()]),
            None
        );
    }

    #[test]
    fn test_auto_allow_safe() {
        let mut system = PermissionSystem::new(None);

        let request = PermissionRequest {
            permission_type: PermissionType::Read,
            path: Some(PathBuf::from("/safe/file.txt")),
            description: "Read safe file".to_string(),
            danger_level: DangerLevel::Safe,
            confirmed: false,
        };

        let result = futures::executor::block_on(system.check(request));
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), PermissionResponse::Allow));
    }

    #[test]
    fn test_dangerous_command_blocked() {
        let mut system = PermissionSystem::new(None);

        let request = PermissionRequest {
            permission_type: PermissionType::Execute,
            path: None,
            description: "rm -rf /".to_string(),
            danger_level: DangerLevel::Critical,
            confirmed: false,
        };

        let result = futures::executor::block_on(system.check(request));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PermissionError::Denied(_)));
    }

    #[test]
    fn test_permission_builder() {
        let system = PermissionSystemBuilder::new()
            .enabled(false)
            .auto_allow_level(DangerLevel::Medium)
            .build();

        assert!(!system.config.enabled);
        assert_eq!(system.config.auto_allow_level, DangerLevel::Medium);
    }

    #[test]
    fn test_danger_levels_order() {
        assert!(DangerLevel::Critical as u8 > DangerLevel::High as u8);
        assert!(DangerLevel::High as u8 > DangerLevel::Medium as u8);
        assert!(DangerLevel::Medium as u8 > DangerLevel::Low as u8);
        assert!(DangerLevel::Low as u8 > DangerLevel::Safe as u8);
    }

    #[test]
    fn test_danger_level_comparisons() {
        assert!(DangerLevel::Safe.le(&DangerLevel::Low));
        assert!(!DangerLevel::Critical.le(&DangerLevel::High));
        assert!(DangerLevel::High.ge(&DangerLevel::Medium));
        assert!(!DangerLevel::Low.ge(&DangerLevel::High));
    }
}
