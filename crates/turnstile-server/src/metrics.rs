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
    // Risk scoring breakdown
    risk_allow: AtomicU64,
    risk_escalate: AtomicU64,
    risk_deny: AtomicU64,
    // Anti-abuse signals
    replay_attempts: AtomicU64,
    challenge_expired: AtomicU64,
    origin_rejected: AtomicU64,
    // Solve-time histogram buckets: <10ms, <50ms, <200ms, <1s, <5s, >=5s
    solve_lt_10ms: AtomicU64,
    solve_lt_50ms: AtomicU64,
    solve_lt_200ms: AtomicU64,
    solve_lt_1s: AtomicU64,
    solve_lt_5s: AtomicU64,
    solve_ge_5s: AtomicU64,
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
    pub fn inc_replay_attempt(&self) {
        self.replay_attempts.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_challenge_expired(&self) {
        self.challenge_expired.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_origin_rejected(&self) {
        self.origin_rejected.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_risk_allow(&self) {
        self.risk_allow.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_risk_escalate(&self) {
        self.risk_escalate.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_risk_deny(&self) {
        self.risk_deny.fetch_add(1, Ordering::Relaxed);
    }
    /// Record a solve time by incrementing the appropriate histogram bucket.
    pub fn record_solve_time(&self, ms: u64) {
        let bucket = if ms < 10 {
            &self.solve_lt_10ms
        } else if ms < 50 {
            &self.solve_lt_50ms
        } else if ms < 200 {
            &self.solve_lt_200ms
        } else if ms < 1000 {
            &self.solve_lt_1s
        } else if ms < 5000 {
            &self.solve_lt_5s
        } else {
            &self.solve_ge_5s
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    /// Render as a Prometheus text exposition (Content-Type `text/plain;
    /// version=0.0.4`).
    pub fn render(&self) -> String {
        let challenges_issued = self.challenges_issued.load(Ordering::Relaxed);
        let verifies_success = self.verifies_success.load(Ordering::Relaxed);
        let verifies_failed = self.verifies_failed.load(Ordering::Relaxed);
        let risk_allow = self.risk_allow.load(Ordering::Relaxed);
        let risk_escalate = self.risk_escalate.load(Ordering::Relaxed);
        let risk_deny = self.risk_deny.load(Ordering::Relaxed);
        let replay_attempts = self.replay_attempts.load(Ordering::Relaxed);
        let challenge_expired = self.challenge_expired.load(Ordering::Relaxed);
        let origin_rejected = self.origin_rejected.load(Ordering::Relaxed);
        let s_lt_10 = self.solve_lt_10ms.load(Ordering::Relaxed);
        let s_lt_50 = self.solve_lt_50ms.load(Ordering::Relaxed);
        let s_lt_200 = self.solve_lt_200ms.load(Ordering::Relaxed);
        let s_lt_1s = self.solve_lt_1s.load(Ordering::Relaxed);
        let s_lt_5s = self.solve_lt_5s.load(Ordering::Relaxed);
        let s_ge_5s = self.solve_ge_5s.load(Ordering::Relaxed);
        format!(
            "# HELP webrify_challenges_issued_total Total challenges minted.\n\
             # TYPE webrify_challenges_issued_total counter\n\
             webrify_challenges_issued_total {challenges_issued}\n\
             # HELP webrify_verifies_total Verify attempts by result.\n\
             # TYPE webrify_verifies_total counter\n\
             webrify_verifies_total{{result=\"success\"}} {verifies_success}\n\
             webrify_verifies_total{{result=\"failed\"}} {verifies_failed}\n\
             # HELP webrify_risk_decisions_total Risk scoring decisions.\n\
             # TYPE webrify_risk_decisions_total counter\n\
             webrify_risk_decisions_total{{decision=\"allow\"}} {risk_allow}\n\
             webrify_risk_decisions_total{{decision=\"escalate\"}} {risk_escalate}\n\
             webrify_risk_decisions_total{{decision=\"deny\"}} {risk_deny}\n\
             # HELP webrify_replay_attempts_total Replay rejections.\n\
             # TYPE webrify_replay_attempts_total counter\n\
             webrify_replay_attempts_total {replay_attempts}\n\
             # HELP webrify_challenge_expired_total Challenges expired before verify.\n\
             # TYPE webrify_challenge_expired_total counter\n\
             webrify_challenge_expired_total {challenge_expired}\n\
             # HELP webrify_origin_rejected_total Origin allowlist rejections.\n\
             # TYPE webrify_origin_rejected_total counter\n\
             webrify_origin_rejected_total {origin_rejected}\n\
             # HELP webrify_solve_time_ms_histogram Solve time distribution (cumulative).\n\
             # TYPE webrify_solve_time_ms_histogram counter\n\
             webrify_solve_time_ms_histogram{{le=\"10\"}} {s_lt_10}\n\
             webrify_solve_time_ms_histogram{{le=\"50\"}} {s_lt_50}\n\
             webrify_solve_time_ms_histogram{{le=\"200\"}} {s_lt_200}\n\
             webrify_solve_time_ms_histogram{{le=\"1000\"}} {s_lt_1s}\n\
             webrify_solve_time_ms_histogram{{le=\"5000\"}} {s_lt_5s}\n\
             webrify_solve_time_ms_histogram{{le=\"+Inf\"}} {s_ge_5s}\n"
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
