use crate::llm::{
    HttpRequestOptions, LlmRunError, http_body_opts, http_plain_opts, http_raw_opts,
    run_codex_jsonl, run_codex_plain, run_ollama_plain, wrap_agent_text_as_jsonl,
};
use crate::runtime::{llm_backend, resolve_ollama_model_for_run};
use base64::Engine;
use serde_json::{Value, json};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendExperimentCapabilities {
    pub turboquant_runtime_support: &'static str,
    pub turboquant_backend_role: &'static str,
    pub turboquant_metric_kind: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTlsPosture {
    pub https_required: bool,
    pub local_http_exception: bool,
    pub allowlist_active: bool,
    pub allowed_hosts: Vec<String>,
    pub pinned_pubkey: bool,
    pub ca_bundle: bool,
    pub client_cert: bool,
    pub client_key: bool,
    pub min_tls_version: &'static str,
    pub follow_redirects: bool,
    pub max_redirects: u32,
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

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
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

fn parse_http_hosts() -> Option<Vec<String>> {
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

fn validate_host_allowlist(url: &str) -> Result<(), LlmRunError> {
    let Some(allowed) = parse_http_hosts() else {
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
        return validate_host_allowlist(trimmed);
    }
    if !trimmed.starts_with("http://") {
        return Err(LlmRunError::message(
            "http-curl adapter [http_url_scheme_invalid] CX_HTTP_PROVIDER_URL must use http:// or https://".to_string(),
        ));
    }
    let require_https = env_bool("CX_HTTP_REQUIRE_HTTPS", true);
    if !require_https {
        return validate_host_allowlist(trimmed);
    }
    let allow_local_http = env_bool("CX_HTTP_ALLOW_LOCAL_HTTP", true);
    if allow_local_http && is_local_url(trimmed) {
        return validate_host_allowlist(trimmed);
    }
    validate_host_allowlist(trimmed)?;
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
        _ => match http_profile() {
            "openai_json" => "json",
            _ => "text",
        },
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
        "json" => match http_profile_opt() {
            Some("openai_json") => Some("openai_chat_completion"),
            _ => Some("json_payload"),
        },
        _ => Some("envelope"),
    }
}

pub fn http_profile_opt() -> Option<&'static str> {
    if selected_provider_transport() != "http" {
        return None;
    }
    Some(http_profile())
}

pub fn http_profile() -> &'static str {
    match env::var("CX_HTTP_REQUEST_PROFILE")
        .ok()
        .map(|v| v.trim().to_lowercase())
        .as_deref()
    {
        Some("openai") | Some("openai-json") | Some("openai_json") => "openai_json",
        _ => "plain_text",
    }
}

pub fn http_auth_mode() -> &'static str {
    match env::var("CX_HTTP_AUTH_PROFILE")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("basic") => "basic",
        Some("header") | Some("custom_header") | Some("custom-header") => "header",
        _ => "bearer",
    }
}

pub fn http_auth_head() -> Option<String> {
    if http_auth_mode() != "header" {
        return None;
    }
    env_nonempty("CX_HTTP_AUTH_HEADER")
}

fn http_auth_pair() -> Result<Option<(String, String)>, LlmRunError> {
    match http_auth_mode() {
        "basic" => {
            let user = env_nonempty("CX_HTTP_AUTH_USERNAME").ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_USERNAME when CX_HTTP_AUTH_PROFILE=basic"
                        .to_string(),
                )
            })?;
            let pass = env_nonempty("CX_HTTP_AUTH_PASSWORD").ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_PASSWORD when CX_HTTP_AUTH_PROFILE=basic"
                        .to_string(),
                )
            })?;
            let creds = format!("{user}:{pass}");
            let enc = base64::engine::general_purpose::STANDARD.encode(creds);
            Ok(Some(("Authorization".to_string(), format!("Basic {enc}"))))
        }
        "header" => {
            let name = http_auth_head().ok_or_else(|| {
                LlmRunError::message(
                    "http-curl adapter requires CX_HTTP_AUTH_HEADER when CX_HTTP_AUTH_PROFILE=header"
                        .to_string(),
                )
            })?;
            let value = env_nonempty("CX_HTTP_AUTH_VALUE")
                .or_else(|| env_nonempty("CX_HTTP_PROVIDER_TOKEN"))
                .ok_or_else(|| {
                    LlmRunError::message(
                        "http-curl adapter requires CX_HTTP_AUTH_VALUE or CX_HTTP_PROVIDER_TOKEN when CX_HTTP_AUTH_PROFILE=header"
                            .to_string(),
                    )
                })?;
            Ok(Some((name, value)))
        }
        _ => Ok(env_nonempty("CX_HTTP_PROVIDER_TOKEN")
            .map(|token| ("Authorization".to_string(), format!("Bearer {token}")))),
    }
}

pub fn http_tlsver() -> &'static str {
    match env::var("CX_HTTP_TLS_MIN_VERSION")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("1.3") | Some("tls1.3") | Some("tlsv1.3") => "1.3",
        Some("default") | Some("system") | Some("system_default") => "default",
        _ => "1.2",
    }
}

pub fn http_follow_redirects() -> bool {
    env_bool("CX_HTTP_FOLLOW_REDIRECTS", false)
}

pub fn http_max_redirects() -> u32 {
    if !http_follow_redirects() {
        return 0;
    }
    env::var("CX_HTTP_MAX_REDIRECTS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(3)
}

pub fn tls_posture_opt() -> Option<HttpTlsPosture> {
    if selected_provider_transport() != "http" {
        return None;
    }
    let allowed_hosts = parse_http_hosts().unwrap_or_default();
    Some(HttpTlsPosture {
        https_required: env_bool("CX_HTTP_REQUIRE_HTTPS", true),
        local_http_exception: env_bool("CX_HTTP_ALLOW_LOCAL_HTTP", true),
        allowlist_active: !allowed_hosts.is_empty(),
        allowed_hosts,
        pinned_pubkey: env_nonempty("CX_HTTP_TLS_PINNEDPUBKEY").is_some(),
        ca_bundle: env_nonempty("CX_HTTP_CA_BUNDLE").is_some(),
        client_cert: env_nonempty("CX_HTTP_CLIENT_CERT").is_some(),
        client_key: env_nonempty("CX_HTTP_CLIENT_KEY").is_some(),
        min_tls_version: http_tlsver(),
        follow_redirects: http_follow_redirects(),
        max_redirects: http_max_redirects(),
    })
}

pub fn tls_posture_json() -> Option<Value> {
    let posture = tls_posture_opt()?;
    Some(json!({
        "https_required": posture.https_required,
        "local_http_exception": posture.local_http_exception,
        "allowlist_active": posture.allowlist_active,
        "allowed_hosts": posture.allowed_hosts,
        "pinned_pubkey": if posture.pinned_pubkey { "set" } else { "off" },
        "ca_bundle": if posture.ca_bundle { "set" } else { "off" },
        "client_cert": if posture.client_cert { "set" } else { "off" },
        "client_key": if posture.client_key { "set" } else { "off" },
        "min_tls_version": posture.min_tls_version,
        "follow_redirects": posture.follow_redirects,
        "max_redirects": posture.max_redirects,
    }))
}

pub fn selected_provider_status() -> Option<&'static str> {
    selected_provider_status_kind().to_log_field()
}

pub fn selected_provider_status_kind() -> ProviderStatus {
    provider_status_for_adapter(selected_adapter_name())
}

pub fn adapter_policy_value() -> Value {
    json!({
        "default_transport": "process",
        "http_transport_opt_in": true,
        "explicit_override_required_for_http": true,
        "selected_adapter": selected_adapter_name(),
        "selected_transport": selected_provider_transport(),
        "selected_status": selected_provider_status_kind().as_str(),
        "http_request_profile": http_profile_opt(),
        "http_auth_mode": if selected_provider_transport() == "http" { Some(http_auth_mode()) } else { None },
        "http_auth_header": http_auth_head(),
        "http_tls_posture": tls_posture_json(),
        "explicit_override_set": adapter_override().is_some(),
        "default_switch_guard": "two_green_ci_windows",
        "rollback_rule": "revert to process default in same release window on schema failures or transport errors"
    })
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

pub fn backend_tq_caps(raw_backend: &str) -> BackendExperimentCapabilities {
    match raw_backend.trim().to_ascii_lowercase().as_str() {
        "mlx" => BackendExperimentCapabilities {
            turboquant_runtime_support: "comparative_only",
            turboquant_backend_role: "comparative_backend",
            turboquant_metric_kind: Some("cache_nbytes"),
        },
        "llama.cpp" | "llamacpp" | "llama_cpp" => BackendExperimentCapabilities {
            turboquant_runtime_support: "reference_only",
            turboquant_backend_role: "codec_reference_backend",
            turboquant_metric_kind: Some("raw_ratio"),
        },
        _ => BackendExperimentCapabilities {
            turboquant_runtime_support: "none",
            turboquant_backend_role: "standard_provider",
            turboquant_metric_kind: None,
        },
    }
}

pub fn selected_tq_caps() -> BackendExperimentCapabilities {
    backend_tq_caps(&llm_backend())
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
    format: HttpProviderFormat,
    request_profile: HttpRequestProfile,
    model: Option<String>,
    http_options: HttpRequestOptions,
}

#[derive(Clone, Copy)]
enum HttpProviderFormat {
    Text,
    Json,
    Jsonl,
}

#[derive(Clone, Copy)]
enum HttpRequestProfile {
    PlainText,
    OpenAiJson,
}

impl HttpCurlAdapter {
    fn parse_format_from_env() -> HttpProviderFormat {
        match selected_http_provider_format() {
            "jsonl" => HttpProviderFormat::Jsonl,
            "json" => HttpProviderFormat::Json,
            _ => HttpProviderFormat::Text,
        }
    }

    fn parse_http_profile() -> HttpRequestProfile {
        match http_profile() {
            "openai_json" => HttpRequestProfile::OpenAiJson,
            _ => HttpRequestProfile::PlainText,
        }
    }

    fn http_model_env() -> Option<String> {
        env::var("CX_HTTP_PROVIDER_MODEL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                let current = crate::runtime::llm_model();
                (!current.trim().is_empty()).then_some(current)
            })
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
                if let Some(payload) = Self::extract_openai_text(&obj)? {
                    return Ok(payload);
                }
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

    fn extract_openai_text(
        obj: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Option<String>, LlmRunError> {
        let Some(choices) = obj.get("choices").and_then(serde_json::Value::as_array) else {
            return Ok(None);
        };
        let Some(message) = choices
            .first()
            .and_then(serde_json::Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(serde_json::Value::as_object)
        else {
            return Ok(None);
        };
        let Some(content) = message.get("content") else {
            return Ok(None);
        };
        match content {
            serde_json::Value::String(s) => Ok(Some(s.to_string())),
            serde_json::Value::Array(items) => {
                let mut joined = Vec::new();
                for item in items {
                    if let Some(text) = item
                        .as_object()
                        .and_then(|v| v.get("text"))
                        .and_then(serde_json::Value::as_str)
                    {
                        joined.push(text.to_string());
                        continue;
                    }
                    if let Some(text) = item.as_str() {
                        joined.push(text.to_string());
                        continue;
                    }
                    return Err(LlmRunError::message(
                        "http-curl adapter [http_openai_content_invalid] unsupported OpenAI content item shape".to_string(),
                    ));
                }
                if joined.is_empty() {
                    return Err(LlmRunError::message(
                        "http-curl adapter [http_openai_content_empty] OpenAI content array had no usable text".to_string(),
                    ));
                }
                Ok(Some(joined.join("\n")))
            }
            _ => Err(LlmRunError::message(
                "http-curl adapter [http_openai_content_type_unsupported] expected OpenAI message.content to be string or array".to_string(),
            )),
        }
    }

    fn run_raw(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.request_profile {
            HttpRequestProfile::PlainText => http_raw_opts(prompt, &self.url, &self.http_options),
            HttpRequestProfile::OpenAiJson => {
                let model = self
                    .model
                    .clone()
                    .unwrap_or_else(|| "xshelf-http".to_string());
                let body = json!({
                    "model": model,
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ],
                    "stream": false
                })
                .to_string();
                http_body_opts(
                    &body,
                    &self.url,
                    "Content-Type: application/json",
                    &self.http_options,
                )
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
        let auth = http_auth_pair()?;
        let tls_pinned_pubkey = env_nonempty("CX_HTTP_TLS_PINNEDPUBKEY");
        let tls_ca_bundle = env_nonempty("CX_HTTP_CA_BUNDLE");
        let tls_client_cert = env_nonempty("CX_HTTP_CLIENT_CERT");
        let tls_client_key = env_nonempty("CX_HTTP_CLIENT_KEY");
        let format = Self::parse_format_from_env();
        let request_profile = Self::parse_http_profile();
        let model = Self::http_model_env();
        Ok(Self {
            url,
            format,
            request_profile,
            model,
            http_options: HttpRequestOptions {
                auth_hdr: auth.as_ref().map(|(name, _)| name.clone()),
                auth_val: auth.as_ref().map(|(_, value)| value.clone()),
                tls_pinned_pubkey,
                tls_ca_bundle,
                tls_client_cert,
                tls_client_key,
                tls_min_version: Some(http_tlsver().to_string()),
                follow_redirects: http_follow_redirects(),
                max_redirects: http_max_redirects(),
            },
        })
    }
}

impl ProviderAdapter for HttpCurlAdapter {
    fn run_plain(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.request_profile {
            HttpRequestProfile::PlainText => http_plain_opts(prompt, &self.url, &self.http_options),
            HttpRequestProfile::OpenAiJson => {
                let raw = self.run_raw(prompt)?;
                Self::extract_json_payload(&raw)
            }
        }
    }

    fn run_jsonl(&self, prompt: &str) -> Result<String, LlmRunError> {
        match self.format {
            HttpProviderFormat::Text => {
                let text = self.run_plain(prompt)?;
                ollama_plain_to_jsonl(&text)
            }
            HttpProviderFormat::Json => {
                let raw = self.run_raw(prompt)?;
                let payload = Self::extract_json_payload(&raw)?;
                ollama_plain_to_jsonl(&payload)
            }
            HttpProviderFormat::Jsonl => {
                let jsonl = self.run_raw(prompt)?;
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
        HttpCurlAdapter, HttpProviderFormat, HttpRequestProfile, ProviderAdapter, ProviderStatus,
        backend_tq_caps, http_profile, is_local_url, normalize_provider_status,
        normalized_backend_name, ollama_plain_to_jsonl, parse_http_hosts, url_host,
        validate_http_url,
    };
    use serde_json::Value;
    use std::env;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env test lock")
    }

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
    fn tq_caps_typed() {
        let standard = backend_tq_caps("codex");
        assert_eq!(standard.turboquant_runtime_support, "none");
        assert_eq!(standard.turboquant_backend_role, "standard_provider");
        assert_eq!(standard.turboquant_metric_kind, None);

        let mlx = backend_tq_caps("mlx");
        assert_eq!(mlx.turboquant_runtime_support, "comparative_only");
        assert_eq!(mlx.turboquant_backend_role, "comparative_backend");
        assert_eq!(mlx.turboquant_metric_kind, Some("cache_nbytes"));

        let llama = backend_tq_caps("llama.cpp");
        assert_eq!(llama.turboquant_runtime_support, "reference_only");
        assert_eq!(llama.turboquant_backend_role, "codec_reference_backend");
        assert_eq!(llama.turboquant_metric_kind, Some("raw_ratio"));
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
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        let err = validate_http_url("http://example.com/v1").expect_err("expected block");
        assert!(err.message.contains("http_url_insecure"), "{}", err.message);
    }

    #[test]
    fn https_allow_local() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_ALLOWED_HOSTS");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("http://127.0.0.1:8080/v1").expect("local http should pass");
    }

    #[test]
    fn https_allow_override() {
        let _guard = env_test_lock();
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
    fn url_host_shapes() {
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
    fn parse_hosts_norm() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var(
                "CX_HTTP_ALLOWED_HOSTS",
                "example.com, EXAMPLE.com,api.local",
            )
        };
        let parsed = parse_http_hosts().expect("hosts");
        assert_eq!(
            parsed,
            vec!["api.local".to_string(), "example.com".to_string()]
        );
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn allowlist_blocks_unknown() {
        let _guard = env_test_lock();
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
    fn allowlist_allows_configured() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_ALLOWED_HOSTS", "allowed.example");
            env::remove_var("CX_HTTP_REQUIRE_HTTPS");
            env::remove_var("CX_HTTP_ALLOW_LOCAL_HTTP");
        }
        validate_http_url("https://allowed.example/v1").expect("must allow configured host");
        unsafe { env::remove_var("CX_HTTP_ALLOWED_HOSTS") };
    }

    #[test]
    fn rollout_policy_ok() {
        let value = super::adapter_policy_value();
        assert_eq!(
            value.get("default_transport").and_then(Value::as_str),
            Some("process")
        );
        assert_eq!(
            value.get("http_transport_opt_in").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            value.get("default_switch_guard").and_then(Value::as_str),
            Some("two_green_ci_windows")
        );
    }

    #[test]
    fn http_profile_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
        assert_eq!(http_profile(), "plain_text");
    }

    #[test]
    fn http_profile_aliases() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai");
        }
        assert_eq!(http_profile(), "openai_json");
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai-json");
        }
        assert_eq!(http_profile(), "openai_json");
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
    }

    #[test]
    fn tlsver_default_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_TLS_MIN_VERSION");
        }
        assert_eq!(super::http_tlsver(), "1.2");
    }

    #[test]
    fn tlsver_alias_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_TLS_MIN_VERSION", "tls1.3");
        }
        assert_eq!(super::http_tlsver(), "1.3");
        unsafe {
            env::set_var("CX_HTTP_TLS_MIN_VERSION", "default");
        }
        assert_eq!(super::http_tlsver(), "default");
        unsafe {
            env::remove_var("CX_HTTP_TLS_MIN_VERSION");
        }
    }

    #[test]
    fn http_redirect_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::remove_var("CX_HTTP_FOLLOW_REDIRECTS");
            env::remove_var("CX_HTTP_MAX_REDIRECTS");
        }
        assert!(!super::http_follow_redirects());
        assert_eq!(super::http_max_redirects(), 0);
    }

    #[test]
    fn redirect_env_cov() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_FOLLOW_REDIRECTS", "1");
            env::set_var("CX_HTTP_MAX_REDIRECTS", "5");
        }
        assert!(super::http_follow_redirects());
        assert_eq!(super::http_max_redirects(), 5);
        unsafe {
            env::remove_var("CX_HTTP_FOLLOW_REDIRECTS");
            env::remove_var("CX_HTTP_MAX_REDIRECTS");
        }
    }

    #[test]
    fn openai_json_extracts() {
        let raw = r#"{"choices":[{"message":{"content":"openai ok"}}]}"#;
        assert_eq!(
            HttpCurlAdapter::extract_json_payload(raw).expect("payload"),
            "openai ok"
        );
    }

    #[test]
    fn openai_json_defaults() {
        let _guard = env_test_lock();
        unsafe {
            env::set_var("CX_HTTP_REQUEST_PROFILE", "openai_json");
            env::remove_var("CX_HTTP_PROVIDER_FORMAT");
        }
        let adapter = HttpCurlAdapter {
            url: "http://127.0.0.1:9999/infer".to_string(),
            format: HttpCurlAdapter::parse_format_from_env(),
            request_profile: HttpRequestProfile::OpenAiJson,
            model: Some("gpt-test".to_string()),
            http_options: Default::default(),
        };
        assert!(matches!(adapter.format, HttpProviderFormat::Json));
        unsafe {
            env::remove_var("CX_HTTP_REQUEST_PROFILE");
        }
    }
}
