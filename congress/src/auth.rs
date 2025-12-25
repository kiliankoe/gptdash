//! HTTP Basic Authentication for host panel

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, Response, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use std::sync::Arc;

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Username for host panel (None = auth disabled)
    pub username: Option<String>,
    /// Password for host panel
    pub password: Option<String>,
}

impl AuthConfig {
    /// Load auth config from environment variables
    /// HOST_USERNAME and HOST_PASSWORD must both be set to enable auth
    pub fn from_env() -> Self {
        let username = std::env::var("HOST_USERNAME")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let password = std::env::var("HOST_PASSWORD")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Both must be set to enable auth
        if username.is_some() && password.is_some() {
            tracing::info!("Host authentication enabled");
            Self { username, password }
        } else {
            if username.is_some() || password.is_some() {
                tracing::warn!(
                    "HOST_USERNAME and HOST_PASSWORD must both be set to enable authentication"
                );
            }
            tracing::warn!("Host authentication DISABLED - anyone can access host panel!");
            Self {
                username: None,
                password: None,
            }
        }
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }

    /// Validate credentials
    pub fn validate(&self, username: &str, password: &str) -> bool {
        match (&self.username, &self.password) {
            (Some(u), Some(p)) => {
                // Use constant-time comparison to prevent timing attacks
                constant_time_eq(u.as_bytes(), username.as_bytes())
                    && constant_time_eq(p.as_bytes(), password.as_bytes())
            }
            _ => true, // Auth disabled, allow all
        }
    }
}

/// Constant-time byte comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// Extract and validate Basic Auth credentials from request headers.
/// Returns true if credentials are valid, false otherwise.
fn validate_basic_auth(request: &Request<Body>, auth_config: &AuthConfig) -> bool {
    let Some(auth_header) = request.headers().get(header::AUTHORIZATION) else {
        return false;
    };
    let Ok(auth_str) = auth_header.to_str() else {
        return false;
    };
    let Some(credentials) = auth_str.strip_prefix("Basic ") else {
        return false;
    };
    let Ok(decoded) = base64_decode(credentials) else {
        return false;
    };
    let Ok(decoded_str) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((username, password)) = decoded_str.split_once(':') else {
        return false;
    };
    auth_config.validate(username, password)
}

/// Middleware for HTTP Basic Authentication on host routes
pub async fn host_auth_middleware(
    State(auth_config): State<Arc<AuthConfig>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // If auth is disabled, pass through
    if !auth_config.is_enabled() {
        return next.run(request).await;
    }

    if validate_basic_auth(&request, &auth_config) {
        return next.run(request).await;
    }

    // Return 401 Unauthorized with WWW-Authenticate header
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::WWW_AUTHENTICATE, "Basic realm=\"GPTDash Host\"")
        .body(Body::from("Unauthorized"))
        .unwrap()
}

/// Simple base64 decoder (avoiding additional dependencies)
fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const DECODE_TABLE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let input = input.trim_end_matches('=');
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;

    for c in input.chars() {
        let c = c as usize;
        if c >= 128 {
            return Err(());
        }
        let value = DECODE_TABLE[c];
        if value < 0 {
            return Err(());
        }
        buffer = (buffer << 6) | (value as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

fn query_param_equals(request: &Request<Body>, key: &str, expected: &str) -> bool {
    let Some(query) = request.uri().query() else {
        return false;
    };
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k == key && v == expected {
            return true;
        }
    }
    false
}

/// Middleware to require HTTP Basic Auth for host and beamer WebSocket connections.
///
/// This prevents clients from taking over by connecting to `/ws?role=host`
/// or cheating by viewing the beamer display via `/ws?role=beamer`.
pub async fn host_ws_auth_middleware(
    State(auth_config): State<Arc<AuthConfig>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let is_host_ws = request.uri().path() == "/ws" && query_param_equals(&request, "role", "host");
    let is_beamer_ws =
        request.uri().path() == "/ws" && query_param_equals(&request, "role", "beamer");

    if !is_host_ws && !is_beamer_ws {
        return next.run(request).await;
    }

    // If host auth is disabled, keep dev behavior (allow) but log loudly.
    if !auth_config.is_enabled() {
        tracing::warn!(
            "Protected WebSocket role requested but host authentication is DISABLED; set HOST_USERNAME and HOST_PASSWORD"
        );
        return next.run(request).await;
    }

    if validate_basic_auth(&request, &auth_config) {
        return next.run(request).await;
    }

    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"GPTDash Host (WebSocket)\"",
        )
        .body(Body::from("Unauthorized"))
        .unwrap()
}

/// Handler to serve host.html with inlined JS (used with auth middleware)
pub async fn serve_host() -> impl IntoResponse {
    match tokio::fs::read_to_string("static/dist/host.html").await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Host page not found"))
            .unwrap(),
    }
}

/// Handler to serve beamer.html with inlined JS
pub async fn serve_beamer() -> impl IntoResponse {
    match tokio::fs::read_to_string("static/dist/beamer.html").await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Beamer page not found"))
            .unwrap(),
    }
}

/// Handler to serve player.html
pub async fn serve_player() -> impl IntoResponse {
    match tokio::fs::read_to_string("static/player.html").await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Player page not found"))
            .unwrap(),
    }
}

/// Handler to serve audience page (index.html)
pub async fn serve_audience() -> impl IntoResponse {
    match tokio::fs::read_to_string("static/index.html").await {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Page not found"))
            .unwrap(),
    }
}

/// Middleware to block audience HTTP requests when panic mode is active
pub async fn panic_mode_middleware(
    State(state): State<Arc<crate::state::AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // This middleware is applied only to audience routes, so just check panic mode
    if state.is_panic_mode().await {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from("Service temporarily unavailable"))
            .unwrap();
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_param_equals() {
        let req = Request::builder()
            .uri("/ws?role=host&token=abc")
            .body(Body::empty())
            .unwrap();
        assert!(query_param_equals(&req, "role", "host"));
        assert!(!query_param_equals(&req, "role", "audience"));
        assert!(!query_param_equals(&req, "missing", "x"));
    }

    #[test]
    fn test_auth_config_disabled_when_incomplete() {
        // Neither set
        let config = AuthConfig {
            username: None,
            password: None,
        };
        assert!(!config.is_enabled());
        assert!(config.validate("any", "thing")); // Passes when disabled

        // Only username set
        let config = AuthConfig {
            username: Some("user".to_string()),
            password: None,
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_auth_config_enabled() {
        let config = AuthConfig {
            username: Some("admin".to_string()),
            password: Some("secret".to_string()),
        };
        assert!(config.is_enabled());
        assert!(config.validate("admin", "secret"));
        assert!(!config.validate("admin", "wrong"));
        assert!(!config.validate("wrong", "secret"));
        assert!(!config.validate("", ""));
    }

    #[test]
    fn test_base64_decode() {
        // "admin:secret" -> "YWRtaW46c2VjcmV0"
        let decoded = base64_decode("YWRtaW46c2VjcmV0").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "admin:secret");

        // "user:pass" -> "dXNlcjpwYXNz"
        let decoded = base64_decode("dXNlcjpwYXNz").unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "user:pass");
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }
}
