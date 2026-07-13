//! End-to-end integration tests for the Wave B Axum server, run against a real
//! Redis at 127.0.0.1:6379 (the service's only external stateful dependency).
//!
//! These exercise the full challenge -> solve -> verify happy path plus the
//! security-critical rejections (replay, disallowed origin, tampered params).

#![forbid(unsafe_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use turnstile_core::pow;
use turnstile_core::protocol::{Challenge, VerifyRequest};
use turnstile_server::{app, config::Config, state::AppState, store::ChallengeStore};

fn test_config() -> Config {
    Config {
        bind_addr: "127.0.0.1:0".into(),
        redis_url: "redis://127.0.0.1:6379/0".into(),
        hmac_key: "integration-hmac-key".into(),
        jwt_key: "integration-jwt-key".into(),
        allowed_origins: vec!["https://example.com".into()],
        // Low difficulty keeps tests fast (~256 iterations average).
        difficulty: 8,
        maxnumber: 100_000,
        challenge_ttl_secs: 300,
        jwt_ttl_secs: 900,
        allow_js_disabled: false,
    }
}

async fn setup() -> AppState {
    let cfg = test_config();
    let store = ChallengeStore::connect(&cfg.redis_url)
        .await
        .expect("Redis must be running at 127.0.0.1:6379");
    AppState::new(cfg, store)
}

async fn body_bytes(body: Body) -> Vec<u8> {
    body.collect().await.unwrap().to_bytes().to_vec()
}

async fn fetch_challenge(app: &App, origin: &str) -> (StatusCode, Option<Challenge>) {
    let req = Request::builder()
        .method("POST")
        .uri("/challenge")
        .header("origin", origin)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = body_bytes(resp.into_body()).await;
    let parsed = serde_json::from_slice::<Challenge>(&bytes).ok();
    (status, parsed)
}

// axum Router type is `axum::Router<()>`; alias for brevity in helpers.
type App = axum::Router<()>;

async fn solve_and_verify(
    app: &axum::Router<()>,
    challenge: &Challenge,
    difficulty_override: Option<u32>,
    nonce_override: Option<u64>,
) -> (StatusCode, serde_json::Value) {
    let seed = hex::decode(&challenge.challenge).unwrap();
    let difficulty = difficulty_override.unwrap_or(challenge.difficulty);
    let nonce = nonce_override.unwrap_or_else(|| pow::solve(&seed, difficulty));

    let vr = VerifyRequest {
        algorithm: challenge.algorithm.clone(),
        challenge: challenge.challenge.clone(),
        salt: challenge.salt.clone(),
        difficulty,
        maxnumber: challenge.maxnumber,
        expires_at: challenge.expires_at,
        origin: challenge.origin.clone(),
        signature: challenge.signature.clone(),
        nonce,
        idempotency_key: format!("idem-{}", challenge.challenge),
        fingerprint: None,
        behavior_score: None,
    };
    let req = Request::builder()
        .method("POST")
        .uri("/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vr).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = body_bytes(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn health_and_ready() {
    let state = setup().await;
    let app = app(state);
    let h = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(h.status(), StatusCode::OK);
    let r = app
        .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn metrics_endpoint_reports_counters() {
    let state = setup().await;
    let app = app(state);
    // Issue + verify once so the counters are non-zero.
    let (_, challenge) = fetch_challenge(&app, "https://example.com").await;
    let challenge = challenge.expect("challenge body");
    let _ = solve_and_verify(&app, &challenge, None, None).await;

    let resp = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = String::from_utf8(body_bytes(resp.into_body()).await).unwrap();
    assert!(text.contains("webrify_challenges_issued_total 1"), "{text}");
    assert!(text.contains("result=\"success\"} 1"), "{text}");
    assert!(text.contains("# TYPE webrify_verifies_total counter"));
}

#[tokio::test]
async fn full_flow_succeeds_and_issues_token() {
    let state = setup().await;
    let app = app(state);
    let (status, challenge) = fetch_challenge(&app, "https://example.com").await;
    assert_eq!(status, StatusCode::OK);
    let challenge = challenge.expect("challenge body");

    let (status, json) = solve_and_verify(&app, &challenge, None, None).await;
    assert_eq!(status, StatusCode::OK, "verify should succeed: {json}");
    assert_eq!(json["success"], true);
    assert!(json["token"].as_str().is_some_and(|t| !t.is_empty()));
}

#[tokio::test]
async fn challenge_rejects_disallowed_origin() {
    let state = setup().await;
    let app = app(state);
    let (status, _) = fetch_challenge(&app, "https://evil.com").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn challenge_rejects_missing_origin() {
    let state = setup().await;
    let app = app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/challenge")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn replay_is_rejected() {
    let state = setup().await;
    let app = app(state);
    let (_, challenge) = fetch_challenge(&app, "https://example.com").await;
    let challenge = challenge.expect("challenge body");

    // First verify succeeds.
    let (s1, _) = solve_and_verify(&app, &challenge, None, None).await;
    assert_eq!(s1, StatusCode::OK);
    // Second verify on the SAME challenge is rejected as replay (409).
    let (s2, _) = solve_and_verify(&app, &challenge, None, None).await;
    assert_eq!(s2, StatusCode::CONFLICT);
}

#[tokio::test]
async fn tampered_difficulty_is_rejected() {
    let state = setup().await;
    let app = app(state);
    let (_, challenge) = fetch_challenge(&app, "https://example.com").await;
    let challenge = challenge.expect("challenge body");

    // Client claims difficulty 2 (easier) — HMAC recompute over the tampered
    // signing string won't match the original signature.
    let (status, _) = solve_and_verify(&app, &challenge, Some(2), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn wrong_nonce_is_rejected() {
    let state = setup().await;
    let app = app(state);
    let (_, challenge) = fetch_challenge(&app, "https://example.com").await;
    let challenge = challenge.expect("challenge body");

    // A nonce that doesn't satisfy the PoW (off by a lot) — note this still
    // passes the HMAC (signature covers params, not the nonce) but fails the
    // PoW re-verify in step 5. Use a deliberately wrong nonce.
    let (status, _) = solve_and_verify(&app, &challenge, None, Some(u64::MAX)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn fingerprint_bound_flow_succeeds() {
    // The client binds its fingerprint into the PoW seed (seed = challenge || fp),
    // solves, and echoes the fingerprint in VerifyRequest. The server must rebuild
    // the SAME seed and accept the solution.
    let state = setup().await;
    let app = app(state);
    let (_, challenge) = fetch_challenge(&app, "https://example.com").await;
    let challenge = challenge.expect("challenge body");

    let fp = turnstile_core::fingerprint::hash(r#"{"canvas":"abc","ua":"test"}"#);
    let fp_hex = hex::encode(fp);
    let mut seed = hex::decode(&challenge.challenge).unwrap();
    seed.extend_from_slice(&fp);
    let nonce = pow::solve(&seed, challenge.difficulty);

    let vr = VerifyRequest {
        algorithm: challenge.algorithm.clone(),
        challenge: challenge.challenge.clone(),
        salt: challenge.salt.clone(),
        difficulty: challenge.difficulty,
        maxnumber: challenge.maxnumber,
        expires_at: challenge.expires_at,
        origin: challenge.origin.clone(),
        signature: challenge.signature.clone(),
        nonce,
        idempotency_key: format!("idem-fp-{}", challenge.challenge),
        fingerprint: Some(fp_hex),
        behavior_score: None,
    };
    let req = Request::builder()
        .method("POST")
        .uri("/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vr).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "fingerprint-bound verify should succeed"
    );
    let bytes = body_bytes(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["success"], true);
}
