//! Risk scoring (v1).
//!
//! Pure, composable function that weighs the available verification signals
//! into a 0-100 risk score and a tri-state [`Decision`]. The server calls this
//! after the deterministic checks (PoW, HMAC, replay) pass; a [`Decision::Deny`]
//! or [`Decision::Escalate`] overrides the default allow.
//!
//! Signals currently weighted: `challenge_passed`, `fingerprint_blacklisted`,
//! `solve_time_ms` (impossibly fast ⇒ bot), `behavior_score` (filled in by
//! Phase 3). Each signal contributes additively, capped at 100. New signals can
//! be added to [`RiskInput`] without changing call sites (forward-compatible
//! via `Option` fields).

/// Inputs to the risk evaluation. All signals beyond `challenge_passed` are
/// optional so the model degrades gracefully as signals are phased in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskInput {
    /// Whether the deterministic checks (PoW, HMAC, replay) already passed.
    pub challenge_passed: bool,
    /// Whether the client's fingerprint appears on a known-bad list.
    pub fingerprint_blacklisted: bool,
    /// Wall-clock time the client took to solve, in milliseconds. `None` if
    /// unmeasured. Impossibly-fast solves indicate precompute / shortcuts.
    pub solve_time_ms: Option<u64>,
    /// Phase-3 behavior score in `[0.0, 1.0]` (higher = more human-like).
    /// `None` until behavior analysis lands.
    pub behavior_score: Option<u32>,
}

/// Tri-state decision derived from the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// `score < ALLOW_CEIL` — verification looks legitimate.
    Allow,
    /// `ALLOW_CEIL <= score < DENY_CEIL` — issue the token but flag for
    /// heightened difficulty / scrutiny on subsequent attempts.
    Escalate,
    /// `score >= DENY_CEIL` — refuse to issue a token.
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskOutput {
    /// Risk score in `[0, 100]` (higher = riskier).
    pub score: u32,
    pub decision: Decision,
}

/// Below this score ⇒ [`Decision::Allow`].
pub const ALLOW_CEIL: u32 = 30;
/// At or above this score ⇒ [`Decision::Deny`].
pub const DENY_CEIL: u32 = 70;

/// Evaluate the risk of an otherwise-valid verification attempt.
pub fn evaluate(input: &RiskInput) -> RiskOutput {
    // A failed challenge is an automatic, maximal-risk deny.
    if !input.challenge_passed {
        return RiskOutput {
            score: 100,
            decision: Decision::Deny,
        };
    }

    let mut score: u32 = 0;

    if input.fingerprint_blacklisted {
        score += 80;
    }
    if let Some(t) = input.solve_time_ms {
        // A real browser needs at least a few JS ticks + a WASM round-trip
        // before it can even start hashing; <20ms is essentially impossible
        // without precompute.
        if t < 20 {
            score += 75;
        } else if t < 50 {
            score += 30;
        }
    }
    if let Some(b) = input.behavior_score {
        // behavior_score stored as basis points in [0, 100] for integer purity
        // (0 = bot-like, 100 = clearly human).
        if b < 30 {
            score += 50;
        } else if b < 60 {
            score += 20;
        }
    }

    let score = score.min(100);
    let decision = if score < ALLOW_CEIL {
        Decision::Allow
    } else if score < DENY_CEIL {
        Decision::Escalate
    } else {
        Decision::Deny
    };
    RiskOutput { score, decision }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean() -> RiskInput {
        RiskInput {
            challenge_passed: true,
            fingerprint_blacklisted: false,
            solve_time_ms: None,
            behavior_score: None,
        }
    }

    #[test]
    fn failed_challenge_always_denies() {
        let out = evaluate(&RiskInput {
            challenge_passed: false,
            ..clean()
        });
        assert_eq!(out.score, 100);
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn clean_attempt_is_allowed() {
        let out = evaluate(&clean());
        assert_eq!(out.score, 0);
        assert_eq!(out.decision, Decision::Allow);
    }

    #[test]
    fn blacklisted_fingerprint_denies() {
        let out = evaluate(&RiskInput {
            fingerprint_blacklisted: true,
            ..clean()
        });
        assert!(out.score >= DENY_CEIL);
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn impossibly_fast_solve_escalates_or_denies() {
        let out = evaluate(&RiskInput {
            solve_time_ms: Some(10),
            ..clean()
        });
        assert!(out.score >= ALLOW_CEIL);
        assert_eq!(out.decision, Decision::Deny);
    }

    #[test]
    fn borderline_fast_solve_escalates() {
        let out = evaluate(&RiskInput {
            solve_time_ms: Some(35),
            ..clean()
        });
        assert_eq!(out.decision, Decision::Escalate);
    }

    #[test]
    fn bot_like_behavior_escalates() {
        let out = evaluate(&RiskInput {
            behavior_score: Some(10),
            ..clean()
        });
        assert!(out.score >= ALLOW_CEIL && out.score < DENY_CEIL);
        assert_eq!(out.decision, Decision::Escalate);
    }

    #[test]
    fn combined_signals_cap_at_100() {
        let out = evaluate(&RiskInput {
            fingerprint_blacklisted: true,
            solve_time_ms: Some(5),
            behavior_score: Some(0),
            ..clean()
        });
        assert_eq!(out.score, 100);
        assert_eq!(out.decision, Decision::Deny);
    }
}
