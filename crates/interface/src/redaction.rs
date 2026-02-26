use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionMode {
    Off,
    Basic,
    Strict,
}

impl RedactionMode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "false" | "0" => Self::Off,
            "strict" | "high" => Self::Strict,
            _ => Self::Basic,
        }
    }

    pub fn from_env() -> Self {
        static MODE: OnceLock<RedactionMode> = OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("NDC_TIMELINE_REDACTION")
                .map(|v| Self::parse(&v))
                .unwrap_or(Self::Basic)
        })
    }
}

pub fn sanitize_text(input: &str, mode: RedactionMode) -> String {
    if mode == RedactionMode::Off {
        return input.to_string();
    }

    static SECRET_ASSIGN_RE: OnceLock<regex::Regex> = OnceLock::new();
    static BEARER_RE: OnceLock<regex::Regex> = OnceLock::new();
    static OPENAI_KEY_RE: OnceLock<regex::Regex> = OnceLock::new();
    static HOME_PATH_RE: OnceLock<regex::Regex> = OnceLock::new();
    static ABS_PATH_RE: OnceLock<regex::Regex> = OnceLock::new();

    let secret_assign = SECRET_ASSIGN_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(api[_-]?key|token|secret|password)\b\s*[:=]\s*([^\s,;]+)")
            .expect("valid secret assign regex")
    });
    let bearer = BEARER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._\-]+\b").expect("valid bearer regex")
    });
    let openai_key = OPENAI_KEY_RE.get_or_init(|| {
        regex::Regex::new(r"\bsk-[A-Za-z0-9]{8,}\b").expect("valid openai key regex")
    });
    let home_path = HOME_PATH_RE
        .get_or_init(|| regex::Regex::new(r"/home/[^/\s]+").expect("valid home path regex"));
    let abs_path = ABS_PATH_RE.get_or_init(|| {
        regex::Regex::new(r"(?P<path>/(?:[^/\s]+/)+[^/\s]+)").expect("valid abs path regex")
    });

    let mut out = input.to_string();
    out = secret_assign.replace_all(&out, "$1=[REDACTED]").to_string();
    out = bearer.replace_all(&out, "Bearer [REDACTED]").to_string();
    out = openai_key.replace_all(&out, "sk-[REDACTED]").to_string();
    out = home_path.replace_all(&out, "/home/***").to_string();

    if mode == RedactionMode::Strict {
        out = abs_path.replace_all(&out, "/***").to_string();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_parse() {
        assert_eq!(RedactionMode::parse("off"), RedactionMode::Off);
        assert_eq!(RedactionMode::parse("strict"), RedactionMode::Strict);
        assert_eq!(RedactionMode::parse("anything"), RedactionMode::Basic);
    }

    #[test]
    fn test_sanitize_text_basic() {
        let input = "token=abc Bearer xyz sk-ABCDEF123456 /home/jerryg/repo";
        let out = sanitize_text(input, RedactionMode::Basic);
        assert!(out.contains("token=[REDACTED]"));
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(out.contains("sk-[REDACTED]"));
        assert!(out.contains("/home/***"));
        assert!(!out.contains("abc"));
    }

    #[test]
    fn test_sanitize_text_off() {
        let input = "token=abc";
        let out = sanitize_text(input, RedactionMode::Off);
        assert_eq!(out, input);
    }

    #[test]
    fn test_sanitize_text_strict_masks_abs_paths() {
        let input = "read /tmp/a/b.txt and /var/log/syslog";
        let out = sanitize_text(input, RedactionMode::Strict);
        assert!(out.contains("/***"));
        assert!(!out.contains("/tmp/a/b.txt"));
        assert!(!out.contains("/var/log/syslog"));
    }
}
