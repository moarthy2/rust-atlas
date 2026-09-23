//! Skill trait + registry.
//!
//! A "Skill" is a small, focused capability unit (in the Claude Skills /
//! MCP-tools sense): name, description (for AI routing), and an async
//! `run` that returns displayable markdown text. The Router Skill asks the
//! custom AI endpoint which skill fits the query; on failure it falls back
//! to keyword heuristics — so the bot works offline.

use std::sync::Arc;

use crate::docs::{DocHit, DocSource, DocStore};

/// Output of a skill run.
#[derive(Debug, Clone)]
pub struct SkillOutput {
    pub source: DocSource,
    pub title: String,
    /// Markdown text ready for Discord chunking.
    pub markdown: String,
    #[allow(dead_code)]
    pub hits: Vec<DocHit>,
}

/// Every doc skill implements this. Object-safe + Send/Sync for registry.
#[async_trait::async_trait]
pub trait Skill: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn source(&self) -> DocSource;
    async fn run(&self, query: &str, store: &DocStore) -> anyhow::Result<SkillOutput>;
}

pub struct DocSkill {
    source: DocSource,
    hints: &'static str,
}

impl DocSkill {
    pub fn new(source: DocSource, hints: &'static str) -> Self {
        Self { source, hints }
    }
}

#[async_trait::async_trait]
impl Skill for DocSkill {
    fn id(&self) -> &'static str {
        self.source.id()
    }
    fn description(&self) -> &'static str {
        self.hints
    }
    fn source(&self) -> DocSource {
        self.source
    }
    async fn run(&self, query: &str, store: &DocStore) -> anyhow::Result<SkillOutput> {
        let hits = store.search(self.source, query, 3).await?;
        if hits.is_empty() {
            anyhow::bail!("no matches in {}", self.source.title());
        }
        let top = &hits[0];
        let md = format_section_markdown(top);
        Ok(SkillOutput {
            source: self.source,
            title: top.section.heading.clone(),
            markdown: md,
            hits,
        })
    }
}

/// Render a DocHit as Discord-friendly markdown.
pub fn format_section_markdown(hit: &DocHit) -> String {
    use crate::docs::slugify;
    let s = &hit.section;
    let anchor = if s.anchor.is_empty() {
        slugify(&s.heading)
    } else {
        s.anchor.clone()
    };
    let url = format!(
        "{}#{anchor}",
        s.source.canonical_url().trim_end_matches('/')
    );
    let mut out = String::new();
    out.push_str(&format!("## {} — {}\n", s.source.title(), s.heading));
    out.push_str(&format!("<{url}>\n\n"));
    out.push_str(&truncate_chars(&s.body, 1800));
    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Registry of the 4 doc skills + the router skill description.
pub struct SkillRegistry {
    pub skills: Vec<Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        let skills: Vec<Arc<dyn Skill>> = vec![
            Arc::new(DocSkill::new(
                DocSource::Reference,
                "authoritative language semantics: syntax, types, lifetimes, keywords, attributes, grammar",
            )),
            Arc::new(DocSkill::new(
                DocSource::ByExample,
                "runnable code examples: hello world, formatting, control flow, functions, modules, generics, error handling",
            )),
            Arc::new(DocSkill::new(
                DocSource::Nomicon,
                "unsafe Rust, FFI, transmuting, raw pointers, aliasing, drop-check, uninitialized memory",
            )),
            Arc::new(DocSkill::new(
                DocSource::Book,
                "gentle beginner guide: ownership, borrowing, structs, enums, cargo, projects, getting started",
            )),
        ];
        Self { skills }
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Skill>> {
        let q = id.to_ascii_lowercase();
        if let Some(src) = DocSource::from_id(&q) {
            let sid = src.id();
            return self.skills.iter().find(|s| s.id() == sid).cloned();
        }
        self.skills.iter().find(|s| s.id() == q).cloned()
    }

    /// Heuristic routing when the AI endpoint is unavailable.
    pub fn heuristic_route(&self, query: &str) -> Arc<dyn Skill> {
        let mut best = &self.skills[0];
        let mut best_score = i32::MIN;
        for s in &self.skills {
            let sc = s.source().heuristic_score(query);
            if sc > best_score {
                best_score = sc;
                best = s;
            }
        }
        // Default to Book for beginner-ish / empty-signal queries.
        if best_score <= 0 {
            return self.get("book").unwrap();
        }
        best.clone()
    }

    /// Compact skill catalogue for the AI system prompt.
    pub fn catalogue(&self) -> String {
        self.skills
            .iter()
            .map(|s| format!("- {}: {}", s.id(), s.description()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
