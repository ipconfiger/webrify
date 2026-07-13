//! Redis-backed challenge store with two interchangeable backends.
//!
//! - [`ChallengeStore::Single`]: one Redis node via `ConnectionManager`
//!   (multiplexed, cheap-clone handle).
//! - [`ChallengeStore::Cluster`]: a Redis Cluster via the async
//!   `ClusterConnection`. All Webrify Redis ops are single-key
//!   (`webrify:spent:{challenge}`, `webrify:escalation:{ip}`), so they're
//!   cluster-safe — no cross-slot multi-key operations or Lua.
//!
//! Anti-replay uses atomic `SET key 1 NX EX` (TOCTOU-safe). Redis failures
//! always propagate so the caller fails CLOSED (verification refused, never
//! silently bypassed).

use std::net::IpAddr;
use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::cluster::ClusterClient;

#[derive(Clone)]
// Both variants live behind `Arc<ChallengeStore>` in `AppState`, so the enum's
// stack size is irrelevant; suppress the variant-size-difference lint.
#[allow(clippy::large_enum_variant)]
pub enum ChallengeStore {
    /// Single-node Redis (ConnectionManager is a cheap-clone handle).
    Single(ConnectionManager),
    /// Redis Cluster. The async cluster connection is a multiplexed handle.
    Cluster(redis::cluster_async::ClusterConnection),
}

impl ChallengeStore {
    /// Connect to a single Redis node.
    pub async fn connect_single(url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let conn = client.get_connection_manager().await?;
        Ok(Self::Single(conn))
    }

    /// Connect to a Redis Cluster. `seed_urls` may be any subset of nodes — the
    /// client discovers the full topology. Pass ≥3 nodes for a healthy cluster.
    pub async fn connect_cluster(seed_urls: &[String]) -> Result<Self, redis::RedisError> {
        let client = ClusterClient::new(seed_urls.to_vec())?;
        let conn = client.get_async_connection().await?;
        Ok(Self::Cluster(conn))
    }

    /// Atomically claim `challenge_key` as spent.
    ///
    /// `Ok(true)` = first claim (proceed); `Ok(false)` = already spent (replay,
    /// reject). Errors propagate so the caller fails closed (HTTP 503).
    pub async fn claim_spent(
        &self,
        challenge_key: &str,
        ttl: Duration,
    ) -> Result<bool, redis::RedisError> {
        match self {
            Self::Single(c) => claim_spent_impl(&mut c.clone(), challenge_key, ttl).await,
            Self::Cluster(c) => claim_spent_impl(&mut c.clone(), challenge_key, ttl).await,
        }
    }

    /// Liveness probe (used by `/ready`).
    pub async fn ping(&self) -> Result<(), redis::RedisError> {
        match self {
            Self::Single(c) => ping_impl(&mut c.clone()).await,
            Self::Cluster(c) => ping_impl(&mut c.clone()).await,
        }
    }

    /// Record a risk-escalation event for `ip` (INCR + refresh TTL). Returns the
    /// new count. Errors propagate (fail-closed on verification; the adaptive
    /// caller skips the bump rather than refusing on a Redis blip).
    pub async fn record_escalation(
        &self,
        ip: IpAddr,
        ttl: Duration,
    ) -> Result<u32, redis::RedisError> {
        match self {
            Self::Single(c) => record_escalation_impl(&mut c.clone(), ip, ttl).await,
            Self::Cluster(c) => record_escalation_impl(&mut c.clone(), ip, ttl).await,
        }
    }

    /// How many recent escalations has this IP accumulated? 0 = clean / unseen.
    pub async fn escalation_count(&self, ip: IpAddr) -> Result<u32, redis::RedisError> {
        match self {
            Self::Single(c) => escalation_count_impl(&mut c.clone(), ip).await,
            Self::Cluster(c) => escalation_count_impl(&mut c.clone(), ip).await,
        }
    }
}

// Generic helpers over any `aio::ConnectionLike` (both ConnectionManager and the
// async ClusterConnection implement it, so the command code is shared).
async fn claim_spent_impl<C: redis::aio::ConnectionLike>(
    conn: &mut C,
    challenge_key: &str,
    ttl: Duration,
) -> Result<bool, redis::RedisError> {
    let key = spent_key(challenge_key);
    let res: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(ttl.as_secs())
        .query_async(conn)
        .await?;
    Ok(res.is_some())
}

async fn ping_impl<C: redis::aio::ConnectionLike>(conn: &mut C) -> Result<(), redis::RedisError> {
    redis::cmd("PING").query_async::<String>(conn).await?;
    Ok(())
}

async fn record_escalation_impl<C: redis::aio::ConnectionLike>(
    conn: &mut C,
    ip: IpAddr,
    ttl: Duration,
) -> Result<u32, redis::RedisError> {
    let key = escalation_key(ip);
    let count: u32 = redis::cmd("INCR").arg(&key).query_async(conn).await?;
    let _: () = redis::cmd("EXPIRE")
        .arg(&key)
        .arg(ttl.as_secs())
        .query_async(conn)
        .await?;
    Ok(count)
}

async fn escalation_count_impl<C: redis::aio::ConnectionLike>(
    conn: &mut C,
    ip: IpAddr,
) -> Result<u32, redis::RedisError> {
    let key = escalation_key(ip);
    let count: Option<u32> = redis::cmd("GET").arg(&key).query_async(conn).await?;
    Ok(count.unwrap_or(0))
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

    /// Integration test against a real single-node Redis at the default URL.
    /// (Cluster mode is exercised manually; CI would spin up a 3-node cluster.)
    async fn connect() -> ChallengeStore {
        ChallengeStore::connect_single("redis://127.0.0.1:6379/0")
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
        assert!(store
            .claim_spent(&key, Duration::from_secs(30))
            .await
            .unwrap());
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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
