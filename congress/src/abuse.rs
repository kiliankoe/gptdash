//! Anti-abuse middleware for basic DDoS protection
//!
//! Provides simple protections for surviving a 90-minute live show:
//! - Blocks curl/wget user agents (basic bot filtering)
//! - Requires X-GPTDash-Client header (JS sets this, curl doesn't)
//! - Rate limiting per token (prevents vote flooding)

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, Response, StatusCode},
    middleware::Next,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Required header that JS clients must send (for HTTP routes, not WebSocket)
pub const REQUIRED_HEADER: &str = "X-GPTDash-Client";
pub const REQUIRED_HEADER_VALUE: &str = "1";

/// Sec-WebSocket-Key header (browsers always send this for WS upgrades)
const SEC_WEBSOCKET_KEY: &str = "sec-websocket-key";

/// Rate limiter state
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Map of IP/token to (request count, window start)
    requests: Arc<RwLock<HashMap<String, (u32, Instant)>>>,
    /// Maximum requests per window
    max_requests: u32,
    /// Time window duration
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(100, Duration::from_secs(10)) // 100 requests per 10 seconds
    }
}

impl RateLimiter {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window,
        }
    }

    /// Check if a request should be allowed
    /// Returns true if allowed, false if rate limited
    pub async fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut requests = self.requests.write().await;

        match requests.get_mut(key) {
            Some((count, window_start)) => {
                // Check if we're in a new window
                if now.duration_since(*window_start) >= self.window {
                    *count = 1;
                    *window_start = now;
                    true
                } else if *count >= self.max_requests {
                    false
                } else {
                    *count += 1;
                    true
                }
            }
            None => {
                requests.insert(key.to_string(), (1, now));
                true
            }
        }
    }

    /// Clean up old entries (call periodically)
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let mut requests = self.requests.write().await;
        requests.retain(|_, (_, window_start)| now.duration_since(*window_start) < self.window * 2);
    }
}

/// Anti-abuse configuration
#[derive(Debug, Clone)]
pub struct AbuseConfig {
    /// Whether to block suspicious user agents
    pub block_user_agents: bool,
    /// Whether to require browser-like headers (Origin for WS, validates it's a real browser)
    pub require_browser_headers: bool,
    /// Rate limiter (None = disabled)
    pub rate_limiter: Option<RateLimiter>,
}

impl Default for AbuseConfig {
    fn default() -> Self {
        Self {
            block_user_agents: true,
            require_browser_headers: true,
            rate_limiter: Some(RateLimiter::default()),
        }
    }
}

impl AbuseConfig {
    /// Load config from environment variables
    pub fn from_env() -> Self {
        let block_user_agents = std::env::var("ABUSE_BLOCK_USER_AGENTS")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true);

        let require_browser_headers = std::env::var("ABUSE_REQUIRE_BROWSER")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true);

        let rate_limit_enabled = std::env::var("ABUSE_RATE_LIMIT")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true);

        let rate_limiter = if rate_limit_enabled {
            let max_requests = std::env::var("ABUSE_RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100);

            let window_secs = std::env::var("ABUSE_RATE_LIMIT_WINDOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10);

            Some(RateLimiter::new(
                max_requests,
                Duration::from_secs(window_secs),
            ))
        } else {
            None
        };

        tracing::info!(
            block_user_agents,
            require_browser_headers,
            rate_limit_enabled,
            "Anti-abuse config loaded"
        );

        Self {
            block_user_agents,
            require_browser_headers,
            rate_limiter,
        }
    }
}

/// Check if a user agent looks like a bot/curl
fn is_blocked_user_agent(user_agent: &str) -> bool {
    let ua_lower = user_agent.to_lowercase();
    // Block common CLI tools
    if ua_lower.contains("curl")
        || ua_lower.contains("wget")
        || ua_lower.contains("httpie")
        || ua_lower.contains("python-requests")
        || ua_lower.contains("python-urllib")
        || ua_lower.contains("libwww-perl")
        || ua_lower.contains("go-http-client")
        || ua_lower.contains("java/")
    {
        return true;
    }

    // Block bots/crawlers (look for "bot" as a word boundary, not substring)
    // e.g., "Googlebot" ends with "bot", "spider" or "crawler" anywhere
    if ua_lower.ends_with("bot")
        || ua_lower.contains("bot/")
        || ua_lower.contains("bot ")
        || ua_lower.contains("spider")
        || ua_lower.contains("crawler")
    {
        return true;
    }

    false
}

/// Extract token from query string for rate limiting
/// Returns None if no token is present (rate limiting skipped for anonymous requests)
fn get_rate_limit_key(request: &Request<Body>) -> Option<String> {
    // Only rate limit by token - not by IP
    // At live events, many audience members share public IPs (venue WiFi)
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                return Some(format!("token:{}", token));
            }
        }
    }
    None
}

/// Build a 403 Forbidden response with message
fn forbidden(message: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(message.to_string()))
        .unwrap()
}

/// Build a 429 Too Many Requests response
fn rate_limited() -> Response<Body> {
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::RETRY_AFTER, "10")
        .body(Body::from("Rate limit exceeded. Please slow down."))
        .unwrap()
}

/// Check if request looks like a browser WebSocket upgrade
fn is_browser_websocket(request: &Request<Body>) -> bool {
    // Browsers always send Sec-WebSocket-Key for WS upgrades
    let has_ws_key = request.headers().contains_key(SEC_WEBSOCKET_KEY);

    // Check for Origin header (browsers send this, curl doesn't by default)
    let has_origin = request.headers().contains_key(header::ORIGIN);

    // Must have both for a legitimate browser WebSocket
    has_ws_key && has_origin
}

/// Middleware for anti-abuse protection on the WebSocket endpoint
pub async fn ws_abuse_middleware(
    State(config): State<Arc<AbuseConfig>>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    // Check user agent
    if config.block_user_agents {
        if let Some(ua) = request.headers().get(header::USER_AGENT) {
            if let Ok(ua_str) = ua.to_str() {
                if is_blocked_user_agent(ua_str) {
                    tracing::warn!(user_agent = ua_str, "Blocked suspicious user agent");
                    return forbidden("Access denied");
                }
            }
        } else {
            // No user agent at all is suspicious
            tracing::warn!("Blocked request with no User-Agent");
            return forbidden("Access denied");
        }
    }

    // Check for browser-like WebSocket request
    if config.require_browser_headers && !is_browser_websocket(&request) {
        tracing::warn!(
            uri = %request.uri(),
            has_origin = request.headers().contains_key(header::ORIGIN),
            has_ws_key = request.headers().contains_key(SEC_WEBSOCKET_KEY),
            "Blocked non-browser WebSocket request"
        );
        return forbidden("Access denied");
    }

    // Check rate limit (only for requests with tokens)
    if let Some(ref rate_limiter) = config.rate_limiter {
        if let Some(key) = get_rate_limit_key(&request) {
            if !rate_limiter.check(&key).await {
                tracing::warn!(key, "Rate limited");
                return rate_limited();
            }
        }
        // Skip rate limiting for requests without tokens (initial connections)
    }

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_user_agents() {
        // Should be blocked
        assert!(is_blocked_user_agent("curl/7.64.1"));
        assert!(is_blocked_user_agent("Wget/1.20.3"));
        assert!(is_blocked_user_agent("python-requests/2.25.1"));
        assert!(is_blocked_user_agent("Python-urllib/3.9"));
        assert!(is_blocked_user_agent("libwww-perl/6.49"));
        assert!(is_blocked_user_agent("Go-http-client/1.1"));
        assert!(is_blocked_user_agent("Java/11.0.11"));
        assert!(is_blocked_user_agent("HTTPie/2.4.0"));
        assert!(is_blocked_user_agent("Googlebot/2.1"));
        assert!(is_blocked_user_agent("bingbot"));
        assert!(is_blocked_user_agent("SomeSpider/1.0"));
        assert!(is_blocked_user_agent("WebCrawler/1.0"));

        // Should be allowed
        assert!(!is_blocked_user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
        ));
        assert!(!is_blocked_user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"
        ));
        assert!(!is_blocked_user_agent(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 14_0 like Mac OS X)"
        ));
        assert!(!is_blocked_user_agent(""));
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_normal_traffic() {
        let limiter = RateLimiter::new(5, Duration::from_secs(1));

        // First 5 requests should pass
        for _ in 0..5 {
            assert!(limiter.check("test-key").await);
        }

        // 6th should be blocked
        assert!(!limiter.check("test-key").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_different_keys() {
        let limiter = RateLimiter::new(2, Duration::from_secs(1));

        // Different keys have separate limits
        assert!(limiter.check("key1").await);
        assert!(limiter.check("key1").await);
        assert!(!limiter.check("key1").await);

        assert!(limiter.check("key2").await);
        assert!(limiter.check("key2").await);
        assert!(!limiter.check("key2").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_window_reset() {
        let limiter = RateLimiter::new(2, Duration::from_millis(50));

        assert!(limiter.check("key").await);
        assert!(limiter.check("key").await);
        assert!(!limiter.check("key").await);

        // Wait for window to reset
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should be allowed again
        assert!(limiter.check("key").await);
    }

    #[test]
    fn test_abuse_config_default() {
        let config = AbuseConfig::default();
        assert!(config.block_user_agents);
        assert!(config.require_browser_headers);
        assert!(config.rate_limiter.is_some());
    }
}
