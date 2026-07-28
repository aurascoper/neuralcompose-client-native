//! Client configuration validation — port of the Expo client's `src/config.ts`.
//! Mode rules: mock is the safe default; live mode with a missing/malformed
//! endpoint is a VISIBLE configuration error, never a silent fallback to mock.
//! Error strings keep the Expo oracle wording (including the env-var names)
//! for parity; shells map their own setting names when rendering.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ClientMode {
    Mock,
    Live,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ResolvedClientConfig {
    pub mode: ClientMode,
    pub server_url: String,
    pub eeg_ws_url: String,
    /// Non-empty when live mode is selected but endpoints are unusable.
    pub config_error: Option<String>,
}

/// "false"/"0"/"no" (any case) select live mode; everything else stays mock.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn parse_use_mock(raw: Option<String>) -> bool {
    match raw {
        None => true,
        Some(v) => !matches!(v.trim().to_lowercase().as_str(), "false" | "0" | "no"),
    }
}

fn normalize_url(raw: Option<&str>) -> String {
    raw.unwrap_or("").trim().trim_end_matches('/').to_string()
}

fn is_http_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Derive the ws(s):// stream URL from the HTTP server URL when not given.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn derive_ws_url(server_url: String) -> String {
    if !is_http_url(&server_url) {
        return String::new();
    }
    // http→ws, https→wss (replace the leading "http" only, like the TS oracle)
    format!("ws{}{}", &server_url[4..], "/api/eeg/stream")
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn resolve_client_mode(
    use_mock_raw: Option<String>,
    server_raw: Option<String>,
    ws_raw: Option<String>,
) -> ResolvedClientConfig {
    let mock = parse_use_mock(use_mock_raw);
    let server_url = normalize_url(server_raw.as_deref());
    let explicit_ws = normalize_url(ws_raw.as_deref());
    let eeg_ws_url = if explicit_ws.is_empty() {
        derive_ws_url(server_url.clone())
    } else {
        explicit_ws
    };

    if mock {
        return ResolvedClientConfig {
            mode: ClientMode::Mock,
            server_url,
            eeg_ws_url,
            config_error: None,
        };
    }
    if !is_http_url(&server_url) {
        let config_error = if server_url.is_empty() {
            "EXPO_PUBLIC_SERVER_URL is not set".to_string()
        } else {
            format!("invalid EXPO_PUBLIC_SERVER_URL: {server_url}")
        };
        return ResolvedClientConfig {
            mode: ClientMode::Live,
            server_url,
            eeg_ws_url,
            config_error: Some(config_error),
        };
    }
    ResolvedClientConfig {
        mode: ClientMode::Live,
        server_url,
        eeg_ws_url,
        config_error: None,
    }
}
