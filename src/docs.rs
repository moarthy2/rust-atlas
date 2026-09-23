//! Doc sources, section model, and in-memory store (moka-bounded).

use std::{sync::Arc, time::Duration};

use moka::future::Cache;
use tracing::{info, warn};

/// A documentation source (one `print.html` page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocSource {
    Reference,
    ByExample,
    Nomicon,
    Book,
}

impl DocSource {
    pub const ALL: [DocSource; 4] = [
        DocSource::Reference,
        DocSource::ByExample,
        DocSource::Nomicon,
        DocSource::Book,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            DocSource::Reference => "reference",
            DocSource::ByExample => "by-example",
            DocSource::Nomicon => "nomicon",
            DocSource::Book => "book",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            DocSource::Reference => "The Rust Reference",
            DocSource::ByExample => "Rust by Example",
            DocSource::Nomicon => "The Rustonomicon",
            DocSource::Book => "The Rust Book",
        }
    }

    pub fn url(&self) -> &'static str {
        match self {
            DocSource::Reference => "https://doc.rust-lang.org/reference/print.html",
            DocSource::ByExample => "https://rust-lang.github.io/rust-by-example/print.html",
            DocSource::Nomicon => "https://doc.rust-lang.org/nomicon/print.html",
            DocSource::Book => "https://doc.rust-lang.org/stable/book/print.html",
        }
    }

    pub fn canonical_url(&self) -> &'static str {
        match self {
            DocSource::Reference => "https://doc.rust-lang.org/reference/",
            DocSource::ByExample => "https://doc.rust-lang.org/rust-by-example/",
            DocSource::Nomicon => "https://doc.rust-lang.org/nomicon/",
            DocSource::Book => "https://doc.rust-lang.org/book/",
        }
    }

    pub fn embed_color(&self) -> u32 {
        match self {
            DocSource::Reference => 0xCE412B,
            DocSource::ByExample => 0xDEA584,
            DocSource::Nomicon => 0x8B0000,
            DocSource::Book => 0xB7410E,
        }
    }

    pub fn from_id(id: &str) -> Option<DocSource> {
        match id.to_ascii_lowercase().as_str() {
            "reference" | "ref" | "spec" => Some(DocSource::Reference),
            "by-example" | "example" | "examples" | "rbe" => Some(DocSource::ByExample),
            "nomicon" | "nomi" => Some(DocSource::Nomicon),
            "book" | "rust-book" | "trpl" => Some(DocSource::Book),
            _ => None,
        }
    }

    /// Heuristic routing used when AI endpoint is unreachable.
    pub fn heuristic_score(&self, query: &str) -> i32 {
        let q = query.to_ascii_lowercase();
        let hit = |words: &[&str]| words.iter().any(|w| q.contains(w));
        match self {
            DocSource::Reference => {
                let mut s = 0;
                if hit(&[
                    "grammar",
                    "syntax",
                    "keyword",
                    "lifetime",
                    "abi",
                    "attribute",
                    "pattern",
                    "expression",
                    "statement",
                    "item",
                    "unsized",
                ]) {
                    s += 3;
                }
                if hit(&["reference", "spec"]) {
                    s += 2;
                }
                s
            }
            DocSource::ByExample => {
                let mut s = 0;
                if hit(&[
                    "example",
                    "hello world",
                    "sample",
                    "how to write",
                    "how do i",
                ]) {
                    s += 3;
                }
                if hit(&["error handling", "generics", "traits", "macros"]) {
                    s += 1;
                }
                s
            }
            DocSource::Nomicon => {
                if hit(&[
                    "unsafe",
                    "raw pointer",
                    "ffi",
                    "transmute",
                    "vtable",
                    "drop check",
                    "uninitialized",
                    "aliasing",
                    "data race",
                    "nomicon",
                ]) {
                    3
                } else {
                    0
                }
            }
            DocSource::Book => {
                if hit(&[
                    "ownership",
                    "borrowing",
                    "borrow checker",
                    "cargo",
                    "struct",
                    "enum",
                    "module",
                    "getting started",
                    "guessing game",
                    "book",
                ]) {
                    3
                } else {
                    0
                }
            }
        }
    }
}

/// One parsed documentation section.
#[derive(Debug, Clone)]
pub struct DocSection {
    pub source: DocSource,
    pub heading: String,
    #[allow(dead_code)]
    pub level: u8,
    pub body: String,
    pub anchor: String,
}

/// Ranked search hit.
#[derive(Debug, Clone)]
pub struct DocHit {
    pub section: DocSection,
    pub score: u32,
}

/// In-memory store with bounded caches.
#[derive(Clone)]
pub struct DocStore {
    http: reqwest::Client,
    raw_cache: Cache<String, Arc<String>>,
    index_cache: Cache<String, Arc<Vec<DocSection>>>,
}

pub const MAX_BODY_CHARS: usize = 3500;
pub const MAX_SECTIONS_PER_SOURCE: usize = 2500;

impl DocStore {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            raw_cache: Cache::builder()
                .max_capacity(4)
                .time_to_live(Duration::from_secs(24 * 3600))
                .build(),
            index_cache: Cache::builder()
                .max_capacity(4)
                .time_to_live(Duration::from_secs(24 * 3600))
                .build(),
        }
    }

    pub async fn preload(&self) {
        for src in DocSource::ALL {
            if let Err(e) = self.sections(src).await {
                warn!(source = src.id(), error = %e, "preload failed");
            } else {
                info!(source = src.id(), "preloaded docs");
            }
        }
    }

    pub async fn raw_html(&self, source: DocSource) -> anyhow::Result<Arc<String>> {
        let key = source.id().to_owned();
        if let Some(v) = self.raw_cache.get(&key).await {
            return Ok(v);
        }
        let text = self
            .http
            .get(source.url())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let arc = Arc::new(text);
        self.raw_cache.insert(key, arc.clone()).await;
        Ok(arc)
    }

    pub async fn sections(&self, source: DocSource) -> anyhow::Result<Arc<Vec<DocSection>>> {
        let key = source.id().to_owned();
        if let Some(v) = self.index_cache.get(&key).await {
            return Ok(v);
        }
        let html = self.raw_html(source).await?;
        let sections = parse_print_html(source, &html);
        let arc = Arc::new(sections);
        self.index_cache.insert(key, arc.clone()).await;
        Ok(arc)
    }

    pub async fn search(
        &self,
        source: DocSource,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<DocHit>> {
        let sections = self.sections(source).await?;
        Ok(search_sections(&sections, query, limit))
    }

    pub async fn search_all(&self, query: &str, limit_per_source: usize) -> Vec<DocHit> {
        let mut out = Vec::new();
        for src in DocSource::ALL {
            match self.search(src, query, limit_per_source).await {
                Ok(mut hits) => out.append(&mut hits),
                Err(e) => warn!(source = src.id(), error = %e, "search failed"),
            }
        }
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }
}
/// Parse mdBook `print.html` into heading-delimited sections.
/// Walks tl nodes in document order; cheap, no selector engine.
pub fn parse_print_html(source: DocSource, html: &str) -> Vec<DocSection> {
    let dom = match tl::parse(html, tl::ParserOptions::default()) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let parser = dom.parser();

    let mut sections: Vec<DocSection> = Vec::with_capacity(512);
    let mut cur_heading = String::from("(intro)");
    let mut cur_level: u8 = 1;
    let mut cur_anchor = String::new();
    let mut cur_body = String::new();

    // Flush helper as closure-free inline fn via macro-like block:
    // (closures borrowing all locals get messy; use explicit code twice)
    for node in dom.nodes().iter() {
        let tl::Node::Tag(tag) = node else { continue };
        let name = tag.name().as_utf8_str().to_ascii_lowercase();
        match name.as_str() {
            "h1" | "h2" | "h3" | "h4" => {
                if sections.len() >= MAX_SECTIONS_PER_SOURCE {
                    break;
                }
                flush_section(
                    source,
                    &mut sections,
                    &mut cur_heading,
                    &mut cur_body,
                    cur_level,
                    &mut cur_anchor,
                );
                let h = tag.inner_text(parser).trim().to_string();
                cur_heading = if h.is_empty() {
                    "(untitled)".to_owned()
                } else {
                    h
                };
                cur_level = match name.as_str() {
                    "h1" => 1,
                    "h2" => 2,
                    "h3" => 3,
                    _ => 4,
                };
                cur_anchor = tag
                    .attributes()
                    .get("id")
                    .flatten()
                    .map(|v| v.as_utf8_str().to_string())
                    .unwrap_or_else(|| slugify(&cur_heading));
            }
            "pre" => {
                let text = tag.inner_text(parser);
                push_bounded(&mut cur_body, text.trim(), MAX_BODY_CHARS);
                push_bounded(&mut cur_body, "\n\n", MAX_BODY_CHARS);
            }
            "p" | "li" | "dt" | "dd" | "blockquote" | "td" | "th" => {
                let text = tag.inner_text(parser);
                let t = text.trim();
                if !t.is_empty() {
                    // Skip if parent pre already captured (avoid dupes):
                    // tl flattens; short guard: only push moderate chunks.
                    push_bounded(&mut cur_body, t, MAX_BODY_CHARS);
                    push_bounded(&mut cur_body, "\n\n", MAX_BODY_CHARS);
                }
            }
            _ => {}
        }
    }
    flush_section(
        source,
        &mut sections,
        &mut cur_heading,
        &mut cur_body,
        cur_level,
        &mut cur_anchor,
    );
    sections
}

fn flush_section(
    source: DocSource,
    sections: &mut Vec<DocSection>,
    heading: &mut String,
    body: &mut String,
    level: u8,
    anchor: &mut String,
) {
    let b = body.trim().to_owned();
    if heading.trim().is_empty() && b.is_empty() {
        return;
    }
    let mut bb = b;
    if bb.len() > MAX_BODY_CHARS {
        let mut end = MAX_BODY_CHARS;
        while !bb.is_char_boundary(end) {
            end -= 1;
        }
        bb.truncate(end);
        bb.push('…');
    }
    sections.push(DocSection {
        source,
        heading: std::mem::take(heading),
        level,
        body: bb,
        anchor: std::mem::take(anchor),
    });
    body.clear();
}

fn push_bounded(buf: &mut String, s: &str, cap: usize) {
    if buf.len() >= cap {
        return;
    }
    let remaining = cap - buf.len();
    if s.len() <= remaining {
        buf.push_str(s);
    } else {
        let mut end = remaining;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        buf.push_str(&s[..end]);
    }
}

pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Fuzzy-rank sections: heading fuzzy score + substring bonuses.
pub fn search_sections(sections: &[DocSection], query: &str, limit: usize) -> Vec<DocHit> {
    use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};

    let query = query.trim();
    if query.is_empty() || sections.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut matcher = Matcher::new(NucleoConfig::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let q_lower = query.to_ascii_lowercase();
    let words: Vec<&str> = q_lower.split_whitespace().collect();

    let mut scored: Vec<DocHit> = Vec::new();
    let mut buf: Vec<char> = Vec::new();
    for sec in sections {
        // Utf32Str::new(str, buf): reuses buf for non-ASCII haystacks.
        let hay = Utf32Str::new(&sec.heading, &mut buf);
        let fuzzy = pattern
            .score(hay, &mut matcher)
            .map(|s| s as u32)
            .unwrap_or(0);
        let h_lower = sec.heading.to_ascii_lowercase();
        let mut bonus: u32 = 0;
        if h_lower.contains(&q_lower) {
            bonus += 5000;
        }
        let word_hits = words.iter().filter(|w| h_lower.contains(*w)).count() as u32;
        bonus += word_hits * 500;
        let mut body_bonus = 0u32;
        if fuzzy == 0 && bonus == 0 {
            let b_lower = sec.body.to_ascii_lowercase();
            if b_lower.contains(&q_lower) {
                body_bonus = 100;
            } else if words.iter().any(|w| b_lower.contains(*w)) {
                body_bonus = 25;
            } else {
                continue;
            }
        }
        let total = fuzzy + bonus + body_bonus;
        if total > 0 {
            scored.push(DocHit {
                section: sec.clone(),
                score: total,
            });
        }
    }
    scored.sort_by(|a, b| b.score.cmp(&a.score));
    scored.truncate(limit);
    scored
}
