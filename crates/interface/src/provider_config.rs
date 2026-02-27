//! Provider Configuration — LLM provider creation and API key resolution.
//!
//! Extracted from `agent_mode.rs` (SEC-S1 God Object refactoring).

use ndc_core::{NdcConfigLoader, ProviderConfig, ProviderType};

/// Returns `true` when `provider` belongs to the MiniMax family of aliases.
pub(crate) fn is_minimax_family(provider: &str) -> bool {
    matches!(
        provider,
        "minimax" | "minimax-coding-plan" | "minimax-cn" | "minimax-cn-coding-plan"
    )
}

/// Returns the base URL for a MiniMax-family provider alias.
pub(crate) fn minimax_base_url(provider: &str) -> &'static str {
    match provider {
        "minimax-cn" | "minimax-cn-coding-plan" => "https://api.minimaxi.com/anthropic/v1",
        _ => "https://api.minimax.io/anthropic/v1",
    }
}

/// Normalises a provider key so that all MiniMax aliases map to `"minimax"`.
pub(crate) fn normalized_provider_key(provider: &str) -> &str {
    if is_minimax_family(provider) {
        "minimax"
    } else {
        provider
    }
}

/// Looks up a provider-specific override in the YAML config, falling back to
/// the normalised key when the exact alias is absent.
pub(crate) fn provider_override_from_config<'a>(
    llm: &'a ndc_core::YamlLlmConfig,
    provider: &str,
) -> Option<&'a ndc_core::YamlProviderConfig> {
    let normalized = normalized_provider_key(provider);
    llm.providers.get(provider).or_else(|| {
        if normalized == provider {
            None
        } else {
            llm.providers.get(normalized)
        }
    })
}

/// Get API key from environment variable with NDC_ prefix, config file, or
/// the generic `NDC_LLM_API_KEY` fallback.
pub(crate) fn get_api_key(provider: &str) -> String {
    let provider_key = normalized_provider_key(provider);
    let env_var = format!("NDC_{}_API_KEY", provider_key.to_uppercase());
    std::env::var(&env_var)
        .ok()
        .or_else(|| {
            let mut loader = NdcConfigLoader::new();
            loader.load().ok()?;
            let llm = loader.config().llm.as_ref()?;

            // provider-specific override
            provider_override_from_config(llm, provider)
                .and_then(|p| p.api_key.clone())
                .or_else(|| llm.api_key.clone())
        })
        .or_else(|| std::env::var("NDC_LLM_API_KEY").ok())
        .unwrap_or_default()
}

/// Get organization/group_id from environment variable or config file.
pub(crate) fn get_organization(provider: &str) -> String {
    let provider_key = normalized_provider_key(provider);
    let env_var = format!("NDC_{}_GROUP_ID", provider_key.to_uppercase());
    std::env::var(&env_var)
        .ok()
        .or_else(|| {
            let mut loader = NdcConfigLoader::new();
            loader.load().ok()?;
            let llm = loader.config().llm.as_ref()?;

            provider_override_from_config(llm, provider)
                .and_then(|p| p.organization.clone())
                .or_else(|| llm.organization.clone())
        })
        .unwrap_or_default()
}

/// Create provider configuration based on provider name.
pub(crate) fn create_provider_config(provider_name: &str, model: &str) -> ProviderConfig {
    let api_key = get_api_key(provider_name);
    let mut organization = get_organization(provider_name);
    let provider_type: ProviderType = if is_minimax_family(provider_name) {
        ProviderType::MiniMax
    } else {
        provider_name.to_string().into()
    };

    let (base_url, models) = match provider_type {
        ProviderType::OpenAi => (
            None,
            vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4".to_string(),
            ],
        ),
        ProviderType::Anthropic => (
            Some("https://api.anthropic.com/v1".to_string()),
            vec![
                "claude-sonnet-4-5-20250929".to_string(),
                "claude-3-5-sonnet".to_string(),
            ],
        ),
        ProviderType::MiniMax => (
            Some(minimax_base_url(provider_name).to_string()),
            vec!["MiniMax-M2.5".to_string(), "MiniMax-M2".to_string()],
        ),
        ProviderType::OpenRouter => (
            Some("https://openrouter.ai/api/v1".to_string()),
            vec![
                "anthropic/claude-3.5-sonnet".to_string(),
                "openai/gpt-4o".to_string(),
            ],
        ),
        ProviderType::Ollama => (
            Some("http://localhost:11434".to_string()),
            vec![
                "llama3.2".to_string(),
                "llama3".to_string(),
                "qwen2.5".to_string(),
            ],
        ),
        _ => (None, vec![model.to_string()]),
    };

    if provider_type == ProviderType::MiniMax {
        // OpenCode compatibility: minimax anthropic endpoint does not need group/org header.
        organization.clear();
    }

    ProviderConfig {
        name: provider_name.to_string(),
        provider_type,
        api_key,
        base_url,
        organization: if organization.is_empty() {
            None
        } else {
            Some(organization)
        },
        default_model: model.to_string(),
        models,
        timeout_ms: 60000,
        max_retries: 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provider_config_minimax_coding_plan() {
        let cfg = create_provider_config("minimax-coding-plan", "MiniMax-M2.5");
        assert_eq!(cfg.provider_type, ProviderType::MiniMax);
        assert_eq!(
            cfg.base_url.as_deref(),
            Some("https://api.minimax.io/anthropic/v1")
        );
        assert_eq!(cfg.default_model, "MiniMax-M2.5");
    }

    #[test]
    fn test_create_provider_config_minimax_cn_coding_plan() {
        let cfg = create_provider_config("minimax-cn-coding-plan", "MiniMax-M2.5");
        assert_eq!(cfg.provider_type, ProviderType::MiniMax);
        assert_eq!(
            cfg.base_url.as_deref(),
            Some("https://api.minimaxi.com/anthropic/v1")
        );
        assert_eq!(cfg.default_model, "MiniMax-M2.5");
    }

    #[test]
    fn test_provider_override_from_config_minimax_alias_fallback() {
        let mut llm = ndc_core::YamlLlmConfig::default();
        llm.providers.insert(
            "minimax".to_string(),
            ndc_core::YamlProviderConfig {
                name: "minimax".to_string(),
                provider_type: "minimax".to_string(),
                model: Some("MiniMax-M2.5".to_string()),
                base_url: None,
                api_key: Some("minimax-key".to_string()),
                organization: Some("group-a".to_string()),
                temperature: None,
                max_tokens: None,
                timeout: None,
                capabilities: None,
            },
        );

        let resolved = provider_override_from_config(&llm, "minimax-cn-coding-plan")
            .expect("fallback to minimax config key");
        assert_eq!(resolved.api_key.as_deref(), Some("minimax-key"));
        assert_eq!(resolved.organization.as_deref(), Some("group-a"));
    }

    #[test]
    fn test_provider_override_from_config_exact_key_preferred() {
        let mut llm = ndc_core::YamlLlmConfig::default();
        llm.providers.insert(
            "minimax".to_string(),
            ndc_core::YamlProviderConfig {
                name: "minimax".to_string(),
                provider_type: "minimax".to_string(),
                model: Some("MiniMax-M2.5".to_string()),
                base_url: None,
                api_key: Some("fallback-key".to_string()),
                organization: Some("group-a".to_string()),
                temperature: None,
                max_tokens: None,
                timeout: None,
                capabilities: None,
            },
        );
        llm.providers.insert(
            "minimax-cn-coding-plan".to_string(),
            ndc_core::YamlProviderConfig {
                name: "minimax-cn-coding-plan".to_string(),
                provider_type: "minimax".to_string(),
                model: Some("MiniMax-M2.5".to_string()),
                base_url: None,
                api_key: Some("alias-key".to_string()),
                organization: Some("group-cn".to_string()),
                temperature: None,
                max_tokens: None,
                timeout: None,
                capabilities: None,
            },
        );

        let resolved = provider_override_from_config(&llm, "minimax-cn-coding-plan")
            .expect("exact alias key should win");
        assert_eq!(resolved.api_key.as_deref(), Some("alias-key"));
        assert_eq!(resolved.organization.as_deref(), Some("group-cn"));
    }
}
