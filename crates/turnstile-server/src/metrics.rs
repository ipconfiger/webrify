//! Lightweight Prometheus-style metrics.
//!
//! No external dependency (the `prometheus`/`metrics` crates are an option
//! later) — just atomic counters + a hand-rendered text exposition format.
//! Exposed at `GET /metrics` for scrape.

use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide counters, shared via `AppState`.
#[derive(Default)]
pub struct Metrics {
    challenges_issued: AtomicU64,
    verifies_success: AtomicU64,
    verifies_failed: AtomicU64,
}

impl Metrics {
    pub fn inc_challenges_issued(&self) {
        self.challenges_issued.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_verifies_success(&self) {
        self.verifies_success.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_verifies_failed(&self) {
        self.verifies_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Render as a Prometheus text exposition (Content-Type `text/plain;
    /// version=0.0.4`).
    pub fn render(&self) -> String {
        let challenges_issued = self.challenges_issued.load(Ordering::Relaxed);
        let verifies_success = self.verifies_success.load(Ordering::Relaxed);
        let verifies_failed = self.verifies_failed.load(Ordering::Relaxed);
        format!(
            "# HELP webrify_challenges_issued_total Total challenges minted.\n\
             # TYPE webrify_challenges_issued_total counter\n\
             webrify_challenges_issued_total {challenges_issued}\n\
             # HELP webrify_verifies_total Verify attempts by result.\n\
             # TYPE webrify_verifies_total counter\n\
             webrify_verifies_total{{result=\"success\"}} {verifies_success}\n\
             webrify_verifies_total{{result=\"failed\"}} {verifies_failed}\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_increment_and_render() {
        let m = Metrics::default();
        m.inc_challenges_issued();
        m.inc_challenges_issued();
        m.inc_verifies_success();
        m.inc_verifies_failed();
        let out = m.render();
        assert!(out.contains("webrify_challenges_issued_total 2"), "{out}");
        assert!(out.contains("result=\"success\"} 1"), "{out}");
        assert!(out.contains("result=\"failed\"} 1"), "{out}");
        assert!(out.contains("# TYPE webrify_verifies_total counter"));
    }

    #[test]
    fn empty_metrics_render_zeroes() {
        let out = Metrics::default().render();
        assert!(out.contains("webrify_challenges_issued_total 0"));
        assert!(out.contains("result=\"success\"} 0"));
    }
}
