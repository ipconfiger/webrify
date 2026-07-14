//! Server configuration, parsed once at startup (Parse-Don't-Validate).
//!
//! Sources (later wins): optional TOML file, then environment variables with a
//! `WEBRIFY_` prefix (nested keys via `__`). `allowed_origins` is supplied as a
//! comma-separated string in either source and parsed into a [`Vec`] here so
//! the rest of the server works with a validated, ready-to-use [`Config`].

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;

/// Validated server configuration. Construct via [`Config::load`].
#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub redis_url: String,
    /// Redis Cluster seed URLs. `Some` → cluster mode; `None` → single-node
    /// (`redis_url`). Comma-separated in env, array in TOML.
    pub cluster_urls: Option<Vec<String>>,
    pub hmac_key: String,
    pub jwt_key: String,
    pub allowed_origins: Vec<String>,
    /// PoW difficulty (leading zero bits). ~14 ≈ 1s on a desktop CPU.
    pub difficulty: u32,
    /// PoW nonce search-space cap.
    pub maxnumber: u64,
    /// Challenge lifetime, seconds.
    pub challenge_ttl_secs: u64,
    /// Issued JWT lifetime, seconds.
    pub jwt_ttl_secs: u64,
    /// If true, JS-disabled clients may pass via a no-PoW high-risk path
    /// (off by default = fail-closed).
    pub allow_js_disabled: bool,
}

impl Config {
    /// Load from an optional TOML file then env (`WEBRIFY_` prefix). Env wins.
    pub fn load(toml_path: Option<&str>) -> Result<Self, ConfigError> {
        let mut fig = Figment::new();
        if let Some(path) = toml_path {
            fig = fig.merge(Toml::file(path));
        }
        fig = fig.merge(Env::prefixed("WEBRIFY_").split("__"));
        let raw: ConfigRaw = fig
            .extract()
            .map_err(|e| ConfigError::Figment(Box::new(e)))?;
        raw.into_config()
    }

    /// Whether `origin` (from a request `Origin` header) may use this service.
    /// `"*"` in the allowlist means all origins are permitted.
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins
            .iter()
            .any(|o| o == "*" || o == origin)
    }
}

/// Intermediate deserialization shape. Exposed only to feed figment; the
/// public API is the validated [`Config`].
#[derive(Debug, Deserialize)]
struct ConfigRaw {
    #[serde(default = "default_bind_addr")]
    bind_addr: String,
    #[serde(default = "default_redis_url")]
    redis_url: String,
    #[serde(default)]
    cluster_urls: Option<CommaList>,
    #[serde(default)]
    hmac_key: String,
    #[serde(default)]
    jwt_key: String,
    /// Comma-separated in env / single string; arrays also accepted from TOML.
    #[serde(default)]
    allowed_origins: CommaList,
    #[serde(default = "default_difficulty")]
    difficulty: u32,
    #[serde(default = "default_maxnumber")]
    maxnumber: u64,
    #[serde(default = "default_challenge_ttl")]
    challenge_ttl_secs: u64,
    #[serde(default = "default_jwt_ttl")]
    jwt_ttl_secs: u64,
    #[serde(default)]
    allow_js_disabled: bool,
}

impl ConfigRaw {
    fn into_config(self) -> Result<Config, ConfigError> {
        if self.hmac_key.is_empty() {
            return Err(ConfigError::Missing("hmac_key"));
        }
        if self.jwt_key.is_empty() {
            return Err(ConfigError::Missing("jwt_key"));
        }
        if self.allowed_origins.0.is_empty() {
            return Err(ConfigError::Missing("allowed_origins"));
        }
        if self.difficulty > 31 {
            return Err(ConfigError::Invalid(format!(
                "difficulty {} too high (max 31 leading-zero bits)",
                self.difficulty
            )));
        }
        Ok(Config {
            bind_addr: self.bind_addr,
            redis_url: self.redis_url,
            cluster_urls: self.cluster_urls.map(|c| c.0),
            hmac_key: self.hmac_key,
            jwt_key: self.jwt_key,
            allowed_origins: self.allowed_origins.0,
            difficulty: self.difficulty,
            maxnumber: self.maxnumber,
            challenge_ttl_secs: self.challenge_ttl_secs,
            jwt_ttl_secs: self.jwt_ttl_secs,
            allow_js_disabled: self.allow_js_disabled,
        })
    }
}

#[derive(Debug, Default)]
struct CommaList(Vec<String>);

impl<'de> serde::Deserialize<'de> for CommaList {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = CommaList;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string of comma-separated origins or a sequence of strings")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<CommaList, E> {
                Ok(CommaList(
                    v.split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect(),
                ))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<CommaList, A::Error> {
                let mut out = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    let s = s.trim().to_owned();
                    if !s.is_empty() {
                        out.push(s);
                    }
                }
                Ok(CommaList(out))
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required config value: {0}")]
    Missing(&'static str),
    #[error("invalid config: {0}")]
    Invalid(String),
    #[error(transparent)]
    Figment(Box<figment::Error>),
}

fn default_bind_addr() -> String {
    "0.0.0.0:3000".into()
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379/0".into()
}
fn default_difficulty() -> u32 {
    14
}
fn default_maxnumber() -> u64 {
    100_000
}
fn default_challenge_ttl() -> u64 {
    300
}
fn default_jwt_ttl() -> u64 {
    900
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::providers::Serialized;

    fn raw(allowed: &str) -> ConfigRaw {
        ConfigRaw {
            bind_addr: default_bind_addr(),
            redis_url: default_redis_url(),
            cluster_urls: None,
            hmac_key: "hk".into(),
            jwt_key: "jk".into(),
            allowed_origins: CommaList(
                allowed
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect(),
            ),
            difficulty: default_difficulty(),
            maxnumber: default_maxnumber(),
            challenge_ttl_secs: default_challenge_ttl(),
            jwt_ttl_secs: default_jwt_ttl(),
            allow_js_disabled: false,
        }
    }

    #[test]
    fn into_config_rejects_empty_keys() {
        let mut r = raw("https://a.com");
        r.hmac_key.clear();
        assert!(matches!(
            r.into_config(),
            Err(ConfigError::Missing("hmac_key"))
        ));
    }

    #[test]
    fn into_config_rejects_empty_origins() {
        let r = raw("  ,  ");
        assert!(matches!(
            r.into_config(),
            Err(ConfigError::Missing("allowed_origins"))
        ));
    }

    #[test]
    fn into_config_rejects_excessive_difficulty() {
        let mut r = raw("https://a.com");
        r.difficulty = 40;
        assert!(matches!(r.into_config(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn is_origin_allowed_matches_allowlist() {
        let cfg = raw("https://a.com, https://b.com").into_config().unwrap();
        assert!(cfg.is_origin_allowed("https://a.com"));
        assert!(cfg.is_origin_allowed("https://b.com"));
        assert!(!cfg.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn load_via_figment_from_serialized() {
        // Round-trip a full Config through figment to exercise env/toml providers.
        let cfg = Config {
            bind_addr: "0.0.0.0:8080".into(),
            redis_url: "redis://db:6379".into(),
            cluster_urls: None,
            hmac_key: "secret".into(),
            jwt_key: "jwt-secret".into(),
            allowed_origins: vec!["https://a.com".into(), "https://b.com".into()],
            difficulty: 12,
            maxnumber: 50_000,
            challenge_ttl_secs: 120,
            jwt_ttl_secs: 600,
            allow_js_disabled: false,
        };
        let fig = Figment::from(Serialized::defaults(serde_json::json!({
            "bind_addr": "0.0.0.0:8080",
            "redis_url": "redis://db:6379",
            "hmac_key": "secret",
            "jwt_key": "jwt-secret",
            "allowed_origins": "https://a.com,https://b.com",
            "difficulty": 12,
            "maxnumber": 50000,
            "challenge_ttl_secs": 120,
            "jwt_ttl_secs": 600,
            "allow_js_disabled": false,
        })));
        let parsed: ConfigRaw = fig.extract().unwrap();
        let cfg2 = parsed.into_config().unwrap();
        assert_eq!(cfg2.allowed_origins, cfg.allowed_origins);
        assert_eq!(cfg2.difficulty, 12);
    }
}
