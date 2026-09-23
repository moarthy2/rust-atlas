//! Custom AI provider endpoint (OpenAI-compatible `/chat/completions`).
//!
//! Used as the **Router Skill**: given a user query, ask the model to pick
//! one of the doc skills (`reference` | `by-example` | `nomicon` | `book`)
//! and optionally rewrite the query for better fuzzy search.
//! Falls back to heuristics when no key/endpoint is configured or on error.

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::skills::SkillRegistry;

#[derive(Debug, Clone)]
pub struct AiProvider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone)]
pub struct RouteDecision {
    pub skill_id: String,
    pub refined_query: String,
    pub via_ai: bool,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    temperature: f32,
    max_tokens: u16,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMsgContent,
}

#[derive(Deserialize)]
struct ChatMsgContent {
    content: Option<String>,
}

impl AiProvider {
    pub fn new(http: reqwest::Client, base_url: String, api_key: String, model: String) -> Self {
        Self {
            http,
            base_url,
            api_key,
            model,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    /// Route a user query to a skill id. Never fails: falls back to heuristic.
    pub async fn route(&self, registry: &SkillRegistry, query: &str) -> RouteDecision {
        if !self.is_configured() {
            let s = registry.heuristic_route(query);
            return RouteDecision {
                skill_id: s.id().to_owned(),
                refined_query: query.to_owned(),
                via_ai: false,
            };
        }
        match self.route_via_ai(registry, query).await {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "AI route failed, heuristic fallback");
                let s = registry.heuristic_route(query);
                RouteDecision {
                    skill_id: s.id().to_owned(),
                    refined_query: query.to_owned(),
                    via_ai: false,
                }
            }
        }
    }

    async fn route_via_ai(
        &self,
        registry: &SkillRegistry,
        query: &str,
    ) -> anyhow::Result<RouteDecision> {
        let catalogue = registry.catalogue();
        let system = format!(
            "You are the Router Skill for a Rust documentation Discord bot.\n\
             Available skills:\n{catalogue}\n\n\
             Reply with ONLY compact JSON: {{\"skill\": \"<id>\", \"query\": \"<refined search query>\"}}.\n\
             skill must be one of: reference, by-example, nomicon, book.\n\
             Refine the query to 2-8 keywords naming the Rust concept (e.g. \"ownership borrowing rules\")."
        );
        let req = ChatRequest {
            model: &self.model,
            temperature: 0.0,
            max_tokens: 120,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: query.to_owned(),
                },
            ],
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp: ChatResponse = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let content = resp
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();
        parse_route_json(&content, registry, query)
    }
}

fn parse_route_json(
    content: &str,
    registry: &SkillRegistry,
    fallback_query: &str,
) -> anyhow::Result<RouteDecision> {
    // Be lenient: model may wrap JSON in code fences.
    let start = content.find('{').unwrap_or(0);
    let end = content.rfind('}').map(|i| i + 1).unwrap_or(content.len());
    let slice = &content[start..end.min(content.len())];
    let v: serde_json::Value = serde_json::from_str(slice)?;
    let skill_raw = v.get("skill").and_then(|s| s.as_str()).unwrap_or("book");
    let q = v
        .get("query")
        .and_then(|s| s.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback_query);
    // Validate skill id, else heuristic.
    let skill_id = if registry.get(skill_raw).is_some() {
        skill_raw.to_ascii_lowercase()
    } else {
        registry.heuristic_route(q).id().to_owned()
    };
    Ok(RouteDecision {
        skill_id,
        refined_query: q.to_owned(),
        via_ai: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillRegistry;

    #[test]
    fn parses_fenced_json() {
        let reg = SkillRegistry::new();
        let d = parse_route_json(
            "```json\n{\"skill\": \"nomicon\", \"query\": \"raw pointers\"}\n```",
            &reg,
            "fallback",
        )
        .unwrap();
        assert_eq!(d.skill_id, "nomicon");
        assert_eq!(d.refined_query, "raw pointers");
        assert!(d.via_ai);
    }

    #[test]
    fn invalid_skill_falls_back_to_heuristic() {
        let reg = SkillRegistry::new();
        let d = parse_route_json(
            "{\"skill\": \"nope\", \"query\": \"unsafe transmute\"}",
            &reg,
            "fallback",
        )
        .unwrap();
        assert_eq!(d.skill_id, "nomicon");
    }
}
