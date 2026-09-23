//! Context7 client: fresh library docs for up-to-date crate info.
//!
//! REST usage (see https://context7.com/docs/api-guide):
//! - GET /api/v3/search?query=..&library=..            (auto-select libs)
//! - GET /api/v2/libs/search?libraryName=..            (resolve library id)
//! - GET /api/v2/context?libraryId=..&query=..         (fetch snippets)
//! All calls use `Authorization: Bearer <key>` and are cached in-memory.
//!
//! This backs `/context7 <library> <query>`: e.g. `/context7 tokio spawn`.

use std::time::Duration;

use moka::future::Cache;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Context7 {
    http: reqwest::Client,
    api_key: Option<String>,
    cache: Cache<String, String>,
}

#[derive(Debug, Deserialize)]
struct LibSearchResponse {
    #[serde(default)]
    results: Vec<LibEntry>,
}

#[derive(Debug, Deserialize)]
struct LibEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct ContextResponse {
    #[serde(default)]
    context: String,
    #[serde(default)]
    snippets: Vec<Snippet>,
}

#[derive(Debug, Deserialize)]
struct Snippet {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

impl Context7 {
    pub fn new(http: reqwest::Client, api_key: Option<String>) -> Self {
        Self {
            http,
            api_key,
            cache: Cache::builder()
                .max_capacity(64)
                .time_to_live(Duration::from_secs(6 * 3600))
                .build(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.api_key.as_ref().is_some_and(|k| !k.is_empty())
    }

    /// Fetch docs context for a library + query. Returns markdown.
    pub async fn fetch(&self, library: &str, query: &str) -> anyhow::Result<String> {
        let key = format!("{library}::{query}");
        if let Some(v) = self.cache.get(&key).await {
            return Ok(v);
        }
        let api_key = self
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| anyhow::anyhow!("CONTEXT7_API_KEY not set"))?;

        // 1) Resolve library id (best-effort; fall back to raw name).
        let lib_id = self
            .resolve_library(&api_key, library)
            .await
            .unwrap_or_else(|| library.to_owned());

        // 2) Get context snippets.
        let md = self.get_context(&api_key, &lib_id, query).await?;
        self.cache.insert(key, md.clone()).await;
        Ok(md)
    }

    async fn resolve_library(&self, api_key: &str, name: &str) -> Option<String> {
        let resp: LibSearchResponse = self
            .http
            .get("https://context7.com/api/v2/libs/search")
            .bearer_auth(api_key)
            .query(&[("libraryName", name)])
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .await
            .ok()?;
        let first = resp.results.into_iter().next()?;
        if first.id.is_empty() {
            if first.name.is_empty() {
                None
            } else {
                Some(first.name)
            }
        } else {
            Some(first.id)
        }
    }

    async fn get_context(
        &self,
        api_key: &str,
        library_id: &str,
        query: &str,
    ) -> anyhow::Result<String> {
        let resp: ContextResponse = self
            .http
            .get("https://context7.com/api/v2/context")
            .bearer_auth(api_key)
            .query(&[("libraryId", library_id), ("query", query)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if !resp.context.is_empty() {
            return Ok(truncate(&resp.context, 1800));
        }
        if resp.snippets.is_empty() {
            anyhow::bail!("no Context7 documentation found");
        }
        let mut out = String::new();
        for sn in resp.snippets.iter().take(3) {
            if !sn.title.is_empty() {
                out.push_str(&format!("### {}\n", sn.title));
            }
            out.push_str(&truncate(&sn.content, 600));
            out.push_str("\n\n");
        }
        Ok(truncate(&out, 1800))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}
