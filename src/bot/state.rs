//! Shared bot state (cheap to clone: Arcs + caches).

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use crate::{
    ai::AiProvider, config::Config, context7::Context7, docs::DocStore, skills::SkillRegistry,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    #[allow(dead_code)]
    pub http: reqwest::Client,
    pub discord: Arc<twilight_http::Client>,
    pub store: DocStore,
    pub skills: Arc<SkillRegistry>,
    pub ai: AiProvider,
    pub context7: Context7,
    /// Paginated pages keyed by interaction token (bounded; 10 min TTL).
    /// Value: Vec<page markdown chunks>.
    pub pages: Cache<String, Arc<Vec<String>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Config::http_timeout())
            .user_agent("rust-docs-bot/0.1")
            .build()
            .expect("reqwest client");
        let discord = Arc::new(twilight_http::Client::new(config.discord_token.clone()));
        let store = DocStore::new(http.clone());
        let ai = AiProvider::new(
            http.clone(),
            config.ai_base_url.clone(),
            config.ai_api_key.clone(),
            config.ai_model.clone(),
        );
        let context7 = Context7::new(http.clone(), config.context7_api_key.clone());
        Self {
            config,
            http,
            discord,
            store,
            skills: Arc::new(SkillRegistry::new()),
            ai,
            context7,
            pages: Cache::builder()
                .max_capacity(256)
                .time_to_live(Duration::from_secs(600))
                .build(),
        }
    }
}
