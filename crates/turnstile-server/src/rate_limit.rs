//! Self-contained per-IP fixed-window rate limiter + axum middleware.
//!
//! No external dependency (the `tower-governor` crate isn't indexed in the
//! rsproxy mirror; rather than fight that, this is a compact in-process
//! limiter). Applied in `main()` only — the test-facing `app()` builder stays
//! unthrottled, since oneshot requests carry no real peer address and the
//! integration tests would otherwise trip the limit.
//!
//! The window is a simple fixed-window counter per source IP. Good enough for
//! abuse mitigation on `/challenge` and `/verify`; a sliding window / token
//! bucket can replace it later behind the same `check` interface.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub struct RateLimiter {
    windows: Mutex<HashMap<IpAddr, Window>>,
    window: Duration,
    max: u32,
}

#[derive(Copy, Clone)]
struct Window {
    start: Instant,
    count: u32,
}

impl RateLimiter {
    /// `max` requests per peer per `window`. e.g. `(Duration::from_secs(1), 10)`
    /// = 10 req/s/IP.
    pub fn new(window: Duration, max: u32) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            window,
            max,
        }
    }

    /// Record a request from `ip` and return `true` if it's within the limit,
    /// `false` if the peer is rate-limited. Recovers from a poisoned lock
    /// (a panic elsewhere) rather than propagating — rate limiting must never
    /// take the server down.
    ///
    /// Periodically evicts expired entries to prevent unbounded memory growth
    /// under sustained abuse from rotating IPs.
    pub fn check(&self, ip: IpAddr) -> bool {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        // Evict expired entries when the map grows large (DoS protection).
        const MAX_ENTRIES: usize = 100_000;
        if windows.len() > MAX_ENTRIES {
            windows.retain(|_, w| now.duration_since(w.start) < self.window);
            // If still too many after eviction, clear entirely — memory safety
            // trumps rate-limit accuracy.
            if windows.len() > MAX_ENTRIES {
                windows.clear();
            }
        }

        let entry = windows.entry(ip).or_insert(Window {
            start: now,
            count: 0,
        });
        if now.duration_since(entry.start) >= self.window {
            entry.start = now;
            entry.count = 0;
        }
        entry.count = entry.count.saturating_add(1);
        entry.count <= self.max
    }
}

/// axum middleware: enforce per-route rate limits by request path.
///
/// State is a tuple of `(challenge_limiter, default_limiter)`. The middleware
/// selects the limiter based on the request path so `/challenge` gets a stricter
/// limit than `/verify` and other routes.
///
/// Wire in `main()`:
/// ```ignore
/// let challenge = Arc::new(RateLimiter::new(Duration::from_secs(1), 3));
/// let default  = Arc::new(RateLimiter::new(Duration::from_secs(1), 10));
/// let app = app(state).layer(from_fn_with_state(
///     (challenge, default),
///     rate_limit_middleware,
/// ));
/// ```
pub async fn rate_limit_middleware(
    State((challenge_limiter, default_limiter)): State<(Arc<RateLimiter>, Arc<RateLimiter>)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let limiter = if req.uri().path() == "/challenge" {
        &challenge_limiter
    } else {
        &default_limiter
    };
    if !limiter.check(addr.ip()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded — slow down",
        )
            .into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(b: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, b))
    }

    #[test]
    fn allows_up_to_max_then_denies_within_window() {
        // Long window so every check lands in the same one.
        let limiter = RateLimiter::new(Duration::from_secs(3600), 2);
        assert!(limiter.check(ip(1))); // 1
        assert!(limiter.check(ip(1))); // 2
        assert!(!limiter.check(ip(1))); // 3 → denied
    }

    #[test]
    fn distinct_ips_are_independent() {
        let limiter = RateLimiter::new(Duration::from_secs(3600), 1);
        assert!(limiter.check(ip(1)));
        assert!(!limiter.check(ip(1))); // same IP exhausted
        assert!(limiter.check(ip(2))); // different IP fine
    }

    #[test]
    fn window_reset_re_allows_again() {
        // Tiny window so a second check after sleep lands in a new window.
        let limiter = RateLimiter::new(Duration::from_millis(1), 1);
        assert!(limiter.check(ip(1)));
        assert!(!limiter.check(ip(1))); // exhausted in this window
        std::thread::sleep(Duration::from_millis(5));
        assert!(limiter.check(ip(1))); // new window → allowed
    }
}
