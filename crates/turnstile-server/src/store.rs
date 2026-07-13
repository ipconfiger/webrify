//! Redis-backed challenge store.
//!
//! Single responsibility: atomically mark a challenge as spent so it can be
//! verified at most once (anti-replay). Uses `SET key 1 NX EX ttl` — a single
//! atomic round-trip — so two concurrent verifies of the same challenge cannot
//! both pass (TOCTOU-safe). Redis failures propagate so the caller fails CLOSED
//! (verification refused, never silently bypassed).

use std::net::IpAddr;
use std::time::Duration;

use redis::aio::ConnectionManager;

pub struct ChallengeStore {
    conn: ConnectionManager,
}

impl ChallengeStore {
    /// Connect (and verify) a pooled async connection manager to `url`.
    pub async fn connect(url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_connection_manager().await?;
        Ok(Self { conn })
    }

    /// Atomically claim `challenge_key` as spent.
    ///
    /// Returns `Ok(true)` if this is the first claim (caller proceeds with
    /// verification), `Ok(false)` if already spent (replay — reject). Any Redis
    /// error propagates so the caller can fail closed (HTTP 503).
    pub async fn claim_spent(
        &self,
        challenge_key: &str,
        ttl: Duration,
    ) -> Result<bool, redis::RedisError> {
        let mut conn = self.conn.clone();
        let key = spent_key(challenge_key);
        // SET key 1 NX EX ttl  →  Some("OK") on first set, None if key existed.
        let res: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs())
            .query_async(&mut conn)
            .await?;
        Ok(res.is_some())
    }

    /// Liveness probe (used by `/ready`).
    pub async fn ping(&self) -> Result<(), redis::RedisError> {
        let mut conn = self.conn.clone();
        redis::cmd("PING").query_async::<String>(&mut conn).await?;
        Ok(())
    }

    /// Record a risk-escalation event for `ip` (used by adaptive difficulty).
    /// Each call INCRements a per-IP counter and refreshes its TTL. Returns the
    /// new count. Redis failures propagate (fail-closed: on error the caller
    /// skips adaptive bump rather than refusing the request).
    pub async fn record_escalation(
        &self,
        ip: IpAddr,
        ttl: Duration,
    ) -> Result<u32, redis::RedisError> {
        let mut conn = self.conn.clone();
        let key = escalation_key(ip);
        let count: u32 = redis::cmd("INCR").arg(&key).query_async(&mut conn).await?;
        let _: () = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl.as_secs())
            .query_async(&mut conn)
            .await?;
        Ok(count)
    }

    /// How many recent escalations has this IP accumulated? 0 = clean (or
    /// never seen, or Redis returned nil for the key).
    pub async fn escalation_count(&self, ip: IpAddr) -> Result<u32, redis::RedisError> {
        let mut conn = self.conn.clone();
        let key = escalation_key(ip);
        let count: Option<u32> = redis::cmd("GET").arg(&key).query_async(&mut conn).await?;
        Ok(count.unwrap_or(0))
    }
}

fn spent_key(challenge_key: &str) -> String {
    format!("webrify:spent:{challenge_key}")
}

fn escalation_key(ip: IpAddr) -> String {
    format!("webrify:escalation:{ip}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration test against a real Redis at the default URL. Requires Redis
    /// running locally (the service's only external stateful dependency).
    async fn connect() -> ChallengeStore {
        ChallengeStore::connect("redis://127.0.0.1:6379/0")
            .await
            .expect("Redis must be running at 127.0.0.1:6379 for this test")
    }

    #[tokio::test]
    async fn ping_works() {
        let store = connect().await;
        store.ping().await.expect("PING should succeed");
    }

    #[tokio::test]
    async fn first_claim_wins_second_is_replay() {
        let store = connect().await;
        let key = format!("replay-test-{}", rand_suffix());
        // First claim succeeds.
        assert!(store
            .claim_spent(&key, Duration::from_secs(30))
            .await
            .unwrap());
        // Second claim on the same key is rejected as replay.
        assert!(!store
            .claim_spent(&key, Duration::from_secs(30))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn distinct_keys_claim_independently() {
        let store = connect().await;
        let k1 = format!("indep-test-{}-a", rand_suffix());
        let k2 = format!("indep-test-{}-b", rand_suffix());
        assert!(store
            .claim_spent(&k1, Duration::from_secs(30))
            .await
            .unwrap());
        assert!(store
            .claim_spent(&k2, Duration::from_secs(30))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn escalation_counter_increments_and_reads() {
        let store = connect().await;
        let ip: IpAddr = format!("198.51.100.{}", rand_suffix() % 200)
            .parse()
            .unwrap();
        assert_eq!(store.escalation_count(ip).await.unwrap(), 0);
        assert_eq!(
            store
                .record_escalation(ip, Duration::from_secs(60))
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .record_escalation(ip, Duration::from_secs(60))
                .await
                .unwrap(),
            2
        );
        assert_eq!(store.escalation_count(ip).await.unwrap(), 2);
    }

    fn rand_suffix() -> u64 {
        // Cheap uniqueness so parallel runs / leftover keys don't collide.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
