//! Shared application state, cloned cheaply into every handler.

use std::sync::Arc;

use crate::config::Config;
use crate::store::ChallengeStore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Arc<ChallengeStore>,
}

impl AppState {
    pub fn new(config: Config, store: ChallengeStore) -> Self {
        Self {
            config: Arc::new(config),
            store: Arc::new(store),
        }
    }
}
