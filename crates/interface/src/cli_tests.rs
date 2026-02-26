//! CLI Tests

#[cfg(test)]
mod tests {
    use crate::cli::{CliError, OutputFormat};
    use std::error::Error;
    use std::path::PathBuf;

    /// Test CliError display implementations
    #[test]
    fn test_cli_error_display() {
        let error = CliError::ExecutorInitFailed("test error".to_string());
        assert_eq!(
            format!("{}", error),
            "Executor initialization failed: test error"
        );

        let error = CliError::StorageError("storage failed".to_string());
        assert_eq!(format!("{}", error), "Storage error: storage failed");
    }

    /// Test CliError source chain
    #[test]
    fn test_cli_error_source() {
        let error = CliError::ExecutorInitFailed("inner error".to_string());
        assert!(error.source().is_none());
    }

    /// Test CliError clones
    #[test]
    fn test_cli_error_clone() {
        let error1 = CliError::ExecutorInitFailed("test".to_string());
        let error2 = error1.clone();
        assert_eq!(format!("{}", error1), format!("{}", error2));
    }

    /// Test OutputFormat enum variants
    #[test]
    fn test_output_format_variants() {
        assert!(matches!(OutputFormat::Pretty, OutputFormat::Pretty));
        assert!(matches!(OutputFormat::Json, OutputFormat::Json));
        assert!(matches!(OutputFormat::Minimal, OutputFormat::Minimal));
    }

    /// Test OutputFormat Copy trait
    #[test]
    fn test_output_format_copy() {
        let format: OutputFormat = OutputFormat::Pretty;
        let copy = format;
        assert!(matches!(copy, OutputFormat::Pretty));
    }

    /// Test CliConfig default
    #[test]
    fn test_cli_config_default() {
        let config = crate::cli::CliConfig::default();

        assert_eq!(config.project_root, PathBuf::from("."));
        assert_eq!(config.storage_path, PathBuf::from(".ndc/storage"));
        assert!(!config.verbose);
        assert_eq!(config.output_format, OutputFormat::Pretty);
    }

    /// Test CliConfig clone
    #[test]
    fn test_cli_config_clone() {
        let config1 = crate::cli::CliConfig::default();
        let config2 = config1.clone();

        assert_eq!(config1.project_root, config2.project_root);
        assert_eq!(config1.storage_path, config2.storage_path);
    }

    /// Test CliConfig with custom values
    #[test]
    fn test_cli_config_custom() {
        let config = crate::cli::CliConfig {
            project_root: PathBuf::from("/custom/path"),
            storage_path: PathBuf::from("/custom/storage"),
            verbose: true,
            output_format: OutputFormat::Json,
        };

        assert_eq!(config.project_root, PathBuf::from("/custom/path"));
        assert_eq!(config.storage_path, PathBuf::from("/custom/storage"));
        assert!(config.verbose);
        assert!(matches!(config.output_format, OutputFormat::Json));
    }

    /// Test Error Debug output
    #[test]
    fn test_cli_error_debug() {
        let error = CliError::ExecutorInitFailed("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("ExecutorInitFailed"));
        assert!(debug_str.contains("test"));
    }

    /// Test partial eq for CliError
    #[test]
    fn test_cli_error_partial_eq() {
        let error1 = CliError::StorageError("same".to_string());
        let error2 = CliError::StorageError("same".to_string());
        let error3 = CliError::StorageError("different".to_string());

        assert_eq!(error1, error2);
        assert_ne!(error1, error3);
    }

    /// Test Send + Sync for CliError
    #[test]
    fn test_cli_error_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CliError>();
    }
}
