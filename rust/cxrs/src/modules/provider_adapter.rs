use crate::llm::{
    HttpRequestOptions, LlmRunError, run_codex_jsonl, run_codex_plain, run_http_plain_with_options,
    run_http_raw_with_options, run_ollama_plain, wrap_agent_text_as_jsonl,
};
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStatus {
    Stable,
    Experimental,
    StubUnimplemented,
}

impl ProviderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Experimental => "experimental",
            Self::StubUnimplemented => "stub_unimplemented",
        }
    }

    pub fn to_log_field(self) -> Option<&'static str> {
        match self {
            Self::Stable => None,
            _ => Some(self.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderCapabilities {
    pub jsonl_native: bool,
    pub schema_strict: bool,
    pub transport: &'static str,
}

fn normalized_backend_name(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("ollama") {
        "ollama"
    } else {
        "codex"
    }
}

fn adapter_override() -> Option<String> {
    env::var("CX_PROVIDER_ADAPTER")
        .ok()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_lowercase())
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn is_local_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://localhost:")
        || u == "http://localhost"
        || u.starts_with("http://localhost/")
        || u.starts_with("http://127.0.0.1:")
        || u == "http://127.0.0.1"
        || u.starts_with("http://127.0.0.1/")
        || u.starts_with("http://[::1]:")
        || u == "http://[::1]"
        || u.starts_with("http://[::1]/")
}

fn url_host(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(without_bracket) = host_port.strip_prefix('[') {
        let host = without_bracket.split(']').next().unwrap_or("").trim();
        return (!host.is_empty()).then(|| host.to_string());
    }
    let host = host_port.split(':').next().unwrap_or("").trim();
    (!host.is_empty()).then(|| host.to_string())
}

fn parse_allowed_http_hosts() -> Option<Vec<String>> {
    let raw = env::var("CX_HTTP_ALLOWED_HOSTS").ok()?;
    let mut hosts: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    hosts.sort();
    hosts.dedup();
    (!hosts.is_empty()).then_some(hosts)
}

fn validate_http_host_allowlist(url: &str) -> Result<(), LlmRunError> {
    let Some(allowed) = parse_allowed_http_hosts() else {
        return Ok(());
    };
    let Some(host) = url_host(url) else {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_host_invalid] unable to parse provider host from CX_HTTP_PROVIDER_URL".to_string(),
        ));
    };
    if allowed.iter().any(|h| h == &host) {
        return Ok(());
    }
    Err(LlmRunError::message(format!(
        "http-curl adapter [http_host_not_allowed] host '{host}' is not in CX_HTTP_ALLOWED_HOSTS"
    )))
}

fn validate_http_url(url: &str) -> Result<(), LlmRunError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_missing] provider URL is empty".to_string(),
        ));
    }
    if trimmed.starts_with("https://") {
        return validate_http_host_allowlist(trimmed);
    }
    if !trimmed.starts_with("http://") {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_scheme_invalid] CX_HTTP_PROVIDER_URL must use http:// or https://".to_string(),
        ));
    }
    let require_https = env_bool("CX_HTTP_REQUIRE_HTTPS", true);
    if !require_https {
        return validate_http_host_allowlist(trimmed);
    }
    let allow_local_http = env_bool("CX_HTTP_ALLOW_LOCAL_HTTP", true);
    if allow_local_http && is_local_url(trimmed) {
        return validate_http_host_allowlist(trimmed);
    }
    validate_http_host_allowlist(trimmed)?;
    Err(LlmRunError::message(
        "http-curl adapter [http_url_insecure] HTTPS is required for non-local endpoints; set CX_HTTP_PROVIDER_URL to https://..., or explicitly set CX_HTTP_REQUIRE_HTTPS=0 for local testing".to_string(),
    ))
}

pub fn selected_adapter_name() -> &'static str {
    if let Some(v) = adapter_override() {
        if v == "mock" {
            return "mock";
        }
        if v == "http-stub" {
            return "http-stub";
        }
        if v == "http" || v == "http-curl" {
            return "http-curl";
        }
    }
    if normalized_backend_name(&llm_backend()) == "ollama" {
        "ollama-cli"
    } else {
        "codex-cli"
    }
}

pub fn selected_provider_transport() -> &'static str {
    provider_transport_for_adapter(selected_adapter_name())
}

pub fn selected_http_provider_format() -> &'static str {
    match env::var("CX_HTTP_PROVIDER_FORMAT")
        .ok()
        .map(|v| v.trim().to_lowercase())
        .as_deref()
    {
        Some("jsonl") => "jsonl",
        Some("json") => "json",
        _ => "text",
    }
}

pub fn selected_http_provider_format_opt() -> Option<&'static str> {
    if selected_provider_transport() != "http" {
        return None;
    }
    Some(selected_http_provider_format())
}

pub fn selected_http_parser_mode_opt() -> Option<&'static str> {
    let format = selected_http_provider_format_opt()?;
    match format {
        "jsonl" => Some("jsonl_passthrough"),
        "json" => Some("json_payload"),
        _ => Some("envelope"),
    }
}

pub fn selected_provider_status() -> Option<&'static str> {
    selected_provider_status_kind().to_log_field()
}

pub fn selected_provider_status_kind() -> ProviderStatus {
    provider_status_for_adapter(selected_adapter_name())
}

pub fn normalize_provider_status(raw: Option<&str>) -> ProviderStatus {
    match raw.map(str::trim).map(str::to_lowercase).as_deref() {
        Some("experimental") => ProviderStatus::Experimental,
        Some("stub_unimplemented") => ProviderStatus::StubUnimplemented,
        _ => ProviderStatus::Stable,
    }
}

fn provider_transport_for_adapter(adapter_name: &str) -> &'static str {
    match adapter_name {
        "mock" => "mock",
        "http-stub" | "http-curl" => "http",
        _ => "process",
    }
}

fn provider_status_for_adapter(adapter_name: &str) -> ProviderStatus {
    match adapter_name {
        "http-stub" => ProviderStatus::StubUnimplemented,
        "http-curl" => ProviderStatus::Experimental,
        _ => ProviderStatus::Stable,
    }
}

pub fn capabilities_for_adapter(adapter_name: &str) -> ProviderCapabilities {
    match adapter_name {
        "codex-cli" => ProviderCapabilities {
            jsonl_native: true,
            schema_strict: true,
            transport: "process",
        },
        "ollama-cli" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
        "mock" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "mock",
        },
        "http-stub" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "http",
        },
        "http-curl" => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "http",
        },
        _ => ProviderCapabilities {
            jsonl_native: false,
            schema_strict: true,
            transport: "process",
        },
    }
}

pub fn selected_provider_capabilities() -> ProviderCapabilities {
    capabilities_for_adapter(selected_adapter_name())
}

pub fn current_provider_capabilities() -> Result<ProviderCapabilities, LlmRunError> {
    let adapter = resolve_provider_adapter()?;
    Ok(adapter.capabilities())
}

fn ollama_plain_to_jsonl(text: &str) -> Result<String, LlmRunError> {
    wrap_agent_text_as_jsonl(text).map_err(LlmRunError::message)
}

pub trait ProviderAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError>;
    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError>;
    fn capabilities(&self) -> ProviderCapabilities;
}

pub struct CodexCliAdapter;

impl ProviderAdapter for CodexCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_codex_plain(prompt)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_codex_jsonl(prompt)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("codex-cli")
    }
}

pub struct OllamaCliAdapter {
    model: String,
}

impl OllamaCliAdapter {
    fn new() -> Result<Self, LlmRunError> {
        let model = resolve_ollama_model_for_run().map_err(LlmRunError::message)?;
        Ok(Self { model })
    }
}

impl ProviderAdapter for OllamaCliAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_ollama_plain(prompt, &self.model)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        let text = self.run_plain(prompt)?;
        ollama_plain_to_jsonl(&text)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("ollama-cli")
    }
}

pub struct MockAdapter {
    plain_response: String,
    jsonl_response: Option<String>,
    error_message: Option<String>,
}

impl MockAdapter {
    fn new_from_env() -> Self {
        let plain_response = env::var("CX_MOCK_PLAIN_RESPONSE")
            .unwrap_or_else(|_| "{\"commands\":[\"echo mock\"]}".to_string());
        let jsonl_response = env::var("CX_MOCK_JSONL_RESPONSE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let error_message = env::var("CX_MOCK_ERROR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            plain_response,
            jsonl_response,
            error_message,
        }
    }
}

impl ProviderAdapter for MockAdapter {
    fn run_plain(&self, _prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        Ok(self.plain_response.clone())
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        if let Some(err) = &self.error_message {
            return Err(LlmRunError::message(err.clone()));
        }
        if let Some(jsonl) = &self.jsonl_response {
            return Ok(jsonl.clone());
        }
        let plain = self.run_plain(prompt)?;
        ollama_plain_to_jsonl(&plain)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("mock")
    }
}

pub struct HttpStubAdapter;

impl ProviderAdapter for HttpStubAdapter {
    fn run_plain(&self, _prompt: &str) -> Result<String, LlmRunError> {
        Err(LlmRunError::message(
            "http-stub adapter selected; HTTP provider transport is not implemented yet"
                .to_string(),
        ))
    }

    fn run_jsonl(&self, _prompt: &str) -> Result<String, LlmRunError> {
        self.run_plain("")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("http-stub")
    }
}

pub struct HttpCurlAdapter {
    url: String,
    token: Option<String>,
    format: HttpProviderFormat,
    http_options: HttpRequestOptions,
}

#[derive(Clone, Copy)]
enum HttpProviderFormat {
    Text,
    Json,
    Jsonl,
}

impl HttpCurlAdapter {
    fn parse_format_from_env() -> HttpProviderFormat {
        let raw = env::var("CX_HTTP_PROVIDER_FORMAT")
            .ok()
            .map(|v| v.trim().to_lowercase())
            .unwrap_or_else(|| "text".to_string());
        match raw.as_str() {
            "jsonl" => HttpProviderFormat::Jsonl,
            "json" => HttpProviderFormat::Json,
            _ => HttpProviderFormat::Text,
        }
    }

    fn extract_json_payload(raw: &str) -> Result<String, LlmRunError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LlmRunError::message(
                "http-curl adapter [http_json_empty] returned empty JSON payload".to_string(),
            ));
        }
        let parsed = serde_json::from_str::<serde_json::Value>(trimmed).map_err(|e| {
            LlmRunError::message(format!(
                "http-curl adapter [http_json_invalid] expected JSON payload: {e}"
            ))
        })?;

        match parsed {
            serde_json::Value::String(s) => Ok(s),
            serde_json::Value::Object(obj) => {
                if let Some(s) = obj.get("text").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(s) = obj.get("response").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(s) = obj.get("output").and_then(serde_json::Value::as_str) {
                    return Ok(s.to_string());
                }
                if let Some(arr) = obj.get("content").and_then(serde_json::Value::as_array) {
                    let mut joined = Vec::new();
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            joined.push(s.to_string());
                            continue;
                        }
                        if let Some(s) = item.get("text").and_then(serde_json::Value::as_str) {
                            joined.push(s.to_string());
                            continue;
                        }
                        return Err(LlmRunError::message(
                            "http-curl adapter [http_json_content_invalid] unsupported content item shape"
                                .to_string(),
                        ));
                    }
                    if joined.is_empty() {
                        return Err(LlmRunError::message(
                            "http-curl adapter [http_json_content_empty] content array had no usable text"
                                .to_string(),
                        ));
                    }
                    return Ok(joined.join("\n"));
                }
                Ok(serde_json::Value::Object(obj).to_string())
            }
            serde_json::Value::Array(_) => Ok(parsed.to_string()),
            serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
                Err(LlmRunError::message(
                    "http-curl adapter [http_json_type_unsupported] expected string/object/array payload"
                        .to_string(),
                ))
            }
        }
    }

    fn validate_jsonl_payload(raw: &str) -> Result<String, LlmRunError> {
        let mut saw_item = false;
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let parsed = serde_json::from_str::<serde_json::Value>(line).map_err(|e| {
                LlmRunError::message(format!("http-curl adapter expected JSONL lines: {e}"))
            })?;
            if parsed.get("type").and_then(serde_json::Value::as_str) == Some("item.completed") {
                saw_item = true;
            }
        }
        if !saw_item {
            return Err(LlmRunError::message(
                "http-curl adapter jsonl payload missing item.completed entry".to_string(),
            ));
        }
        Ok(raw.to_string())
    }

    fn new_from_env() -> Result<Self, LlmRunError> {
        let url = env::var("CX_HTTP_PROVIDER_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_PROVIDER_URL to be set".to_string(),
                )
            })?;
        validate_http_url(&url)?;
        let token = env::var("CX_HTTP_PROVIDER_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let tls_pinned_pubkey = env::var("CX_HTTP_TLS_PINNEDPUBKEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let format = Self::parse_format_from_env();
        Ok(Self {
            url,
            token,
            format,
            http_options: HttpRequestOptions { tls_pinned_pubkey },
        })
    }
}

impl ProviderAdapter for HttpCurlAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        run_http_plain_with_options(prompt, &self.url, self.token.as_deref(), &self.http_options)
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.format {
            HttpProviderFormat::Text => {
                let text = self.run_plain(prompt)?;
                ollama_plain_to_jsonl(&text)
            }
            HttpProviderFormat::Json => {
                let raw = run_http_raw_with_options(
                    prompt,
                    &self.url,
                    self.token.as_deref(),
                    &self.http_options,
                )?;
                let payload = Self::extract_json_payload(&raw)?;
                ollama_plain_to_jsonl(&payload)
            }
            HttpProviderFormat::Jsonl => {
                let jsonl = run_http_raw_with_options(
                    prompt,
                    &self.url,
                    self.token.as_deref(),
                    &self.http_options,
                )?;
                Self::validate_jsonl_payload(&jsonl)
            }
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities_for_adapter("http-curl")
    }
}

pub fn resolve_provider_adapter() -> Result<Box<dyn ProviderAdapter>, LlmRunError> {
    if let Some(v) = adapter_override() {
        if v == "mock" {
            return Ok(Box::new(MockAdapter::new_from_env()));
        }
        if v == "http-stub" {
            return Ok(Box::new(HttpStubAdapter));
        }
        if v == "http" || v == "http-curl" {
            return Ok(Box::new(HttpCurlAdapter::new_from_env()?));
        }
    }
    if normalized_backend_name(&llm_backend()) == "ollama" {
        return Ok(Box::new(OllamaCliAdapter::new()?));
    }
    Ok(Box::new(CodexCliAdapter))
}

pub fn run_jsonl_with_current_adapter(prompt: &str) -> Result<String, LlmRunError> {
    let adapter = resolve_provider_adapter()?;
    adapter.run_jsonl(prompt)
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderAdapter, ProviderStatus, is_local_url, normalize_provider_status,
        normalized_backend_name, ollama_plain_to_jsonl, parse_allowed_http_hosts, url_host,
        validate_http_url,
    };
    use serde_json::Value;
    use std::env;

    #[test]
    fn backend_normalization_defaults_to_codex() {
        assert_eq!(normalized_backend_name("codex"), "codex");
        assert_eq!(normalized_backend_name("CoDeX"), "codex");
        assert_eq!(normalized_backend_name("unknown"), "codex");
    }

    #[test]
    fn backend_normalization_accepts_ollama_case() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("OLLAMA"), "ollama");
    }

    #[test]
    fn ollama_plain_output_wrapped_as_jsonl_agent() {
        let raw = "line1\nline2 with \"quotes\"";
        let jsonl = ollama_plain_to_jsonl(raw).expect("wrap jsonl");
        let parsed: Value = serde_json::from_str(&jsonl).expect("parse wrapped json");
        assert_eq!(
            parsed.get("type").and_then(Value::as_str),
            Some("item.completed")
        );
        let item = parsed.get("item").expect("item");
        assert_eq!(
            item.get("type").and_then(Value::as_str),
            Some("agent_message")
        );
        assert_eq!(item.get("text").and_then(Value::as_str), Some(raw));
    }

    #[test]
    fn selected_adapter_name_follows_backend_normalization() {
        assert_eq!(normalized_backend_name("ollama"), "ollama");
        assert_eq!(normalized_backend_name("codex"), "codex");
    }

    #[test]
    fn provider_transport_mapping_covers_mock_and_process() {
        assert_eq!(super::provider_transport_for_adapter("mock"), "mock");
        assert_eq!(
            super::provider_transport_for_adapter("codex-cli"),
            "process"
        );
        assert_eq!(
            super::provider_transport_for_adapter("ollama-cli"),
            "process"
        );
    }

    #[test]
    fn capabilities_mapping_is_deterministic() {
        let codex = super::capabilities_for_adapter("codex-cli");
        assert!(codex.jsonl_native);
        assert!(codex.schema_strict);
        assert_eq!(codex.transport, "process");

        let ollama = super::capabilities_for_adapter("ollama-cli");
        assert!(!ollama.jsonl_native);
        assert!(ollama.schema_strict);
        assert_eq!(ollama.transport, "process");

        let mock = super::capabilities_for_adapter("mock");
        assert!(!mock.jsonl_native);
        assert!(mock.schema_strict);
        assert_eq!(mock.transport, "mock");

        let http = super::capabilities_for_adapter("http-stub");
        assert!(!http.jsonl_native);
        assert!(http.schema_strict);
        assert_eq!(http.transport, "http");

        let http_curl = super::capabilities_for_adapter("http-curl");
        assert!(!http_curl.jsonl_native);
        assert!(http_curl.schema_strict);
        assert_eq!(http_curl.transport, "http");
    }

    #[test]
    fn adapter_trait_capabilities_match_mapping() {
        let codex = super::CodexCliAdapter;
        let caps = codex.capabilities();
        assert!(caps.jsonl_native);
        assert_eq!(caps.transport, "process");
    }

    #[test]
    fn adapter_override_http_stub_sets_transport_and_status() {
        assert_eq!(super::provider_transport_for_adapter("http-stub"), "http");
        assert_eq!(
            super::provider_status_for_adapter("http-stub"),
            ProviderStatus::StubUnimplemented
        );
        assert_eq!(super::provider_transport_for_adapter("http-curl"), "http");
        assert_eq!(
            super::provider_status_for_adapter("http-curl"),
            ProviderStatus::Experimental
        );
        assert_eq!(
            super::provider_status_for_adapter("codex-cli"),
            ProviderStatus::Stable
        );
    }

    #[test]
    fn normalize_provider_status_maps_unknown_to_stable() {
        assert_eq!(
            normalize_provider_status(Some("experimental")),
            ProviderStatus::Experimental
        );
        assert_eq!(
            normalize_provider_status(Some("stub_unimplemented")),
            ProviderStatus::StubUnimplemented
        );
        assert_eq!(
            normalize_provider_status(Some("totally_unknown")),
            ProviderStatus::Stable
        );
        assert_eq!(normalize_provider_status(None), ProviderStatus::Stable);
    }

    #[test]
    fn local_url_strict() {
        assert!(is_local_url("http://localhost:8080/v1"));
        assert!(is_local_url("http://127.0.0.1"));
        assert!(is_local_url("http://[::1]/health"));
        assert!(!is_local_url("http://example.com"));
        assert!(!is_local_url("https://localhost:8080"));
    }

    #[test]
    fn https_block_default() {
        unsafe {
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        let err = validate_http_url("http://example.com/v1").expect_err("expected block");
        assert!(err.message.contains("http_url_insecure"), "{}", err.message);
    }

    #[test]
    fn https_allow_local() {
        unsafe {
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("http://127.0.0.1:8080/v1").expect("local http should pass");
    }

    #[test]
    fn https_allow_override() {
        unsafe {
            env::set_var("CX_HTTP_REQUIRE_HTTPS", "0");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
        }
        validate_http_url("http://example.com/v1").expect("https override should allow");
        unsafe {
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
        };
    }

    #[test]
    fn parse_url_host_handles_standard_shapes() {
        assert_eq!(
            url_host("https://example.com/v1").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            url_host("http://127.0.0.1:8080").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            url_host("https://user:pw@api.example.com/x").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(url_host("https://[::1]:9443").as_deref(), Some("::1"));
    }

    #[test]
    fn allowed_hosts_parser_normalizes_and_deduplicates() {
        unsafe {
            env::set_var(
                "CX_HTTP_ALLOWED_HOSTS",
                "example.com, EXAMPLE.com,api.local",
            )
        };
        let parsed = parse_allowed_http_hosts().expect("hosts");
        assert_eq!(
            parsed,
            vec!["api.local".to_string(), "example.com".to_string()]
        );
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn allowlist_blocks_unknown_hosts() {
        unsafe {
            env::set_var("CX_HTTP_ALLOWED_HOSTS", "allowed.example");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        let err = validate_http_url("https://blocked.example/v1").expect_err("must block");
        assert!(
            err.message.contains("http_host_not_allowed"),
            "{}",
            err.message
        );
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn allowlist_allows_configured_hosts() {
        unsafe {
            env::set_var("CX_HTTP_ALLOWED_HOSTS", "allowed.example");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("https://allowed.example/v1").expect("must allow configured host");
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }
}
