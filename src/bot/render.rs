//! Discord rendering helpers: embeds + chunking + pagination buttons.
//!
//! Discord limits (handled here):
//! - message content <= 2000 chars
//! - embed description <= 4096 chars, title <= 256, footer <= 2048
//! - components: max 5 action rows, 5 buttons per row

use twilight_model::channel::message::{
    Component,
    component::{ActionRow, Button, ButtonStyle},
    embed::{Embed, EmbedFooter},
};

/// Split markdown into <= limit-char chunks on char boundaries,
/// preferring newline splits for readability.
pub fn chunk_markdown(md: &str, limit: usize) -> Vec<String> {
    if md.chars().count() <= limit {
        return vec![md.to_owned()];
    }
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for line in md.split_inclusive('\n') {
        let l = line.chars().count();
        if cur_len + l > limit && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_len = 0;
        }
        // Single huge line: hard-split.
        if l > limit {
            let mut buf = String::new();
            let mut n = 0;
            for c in line.chars() {
                buf.push(c);
                n += 1;
                if n >= limit {
                    if !cur.is_empty() {
                        chunks.push(std::mem::take(&mut cur));
                        cur_len = 0;
                    }
                    chunks.push(std::mem::take(&mut buf));
                    n = 0;
                }
            }
            cur.push_str(&buf);
            cur_len += n;
        } else {
            cur.push_str(line);
            cur_len += l;
        }
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

pub fn build_embed(
    title: &str,
    description: &str,
    color: u32,
    footer: &str,
    url: Option<&str>,
) -> Embed {
    Embed {
        author: None,
        color: Some(color),
        description: Some(truncate(description, 4096)),
        fields: Vec::new(),
        footer: Some(EmbedFooter {
            text: footer.to_owned(),
            icon_url: None,
            proxy_icon_url: None,
        }),
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: Some(truncate(title, 256)),
        url: url.map(|s| s.to_owned()),
        video: None,
    }
}

/// Pagination row: `docs:prev:<idx>:<total>` / `docs:next:<idx>:<total>`.
/// Page state itself is re-derived from the embed content? No — we encode
/// chunk index in custom_id and keep chunks server-side? For zero-state
/// simplicity: buttons carry target page; handler re-runs lookup via cache.
/// Here we just build prev/next buttons; bot stores pages in moka cache
/// keyed by message id (see bot::pages).
pub fn pagination_row(page: usize, total: usize) -> Vec<Component> {
    // Encode: docs:<dir>:<current_page>. Handler derives target page.
    let prev = Component::Button(Button {
        id: None,
        custom_id: Some(format!("docs:prev:{page}")),
        disabled: page == 0,
        emoji: None,
        label: Some("< Prev".to_owned()),
        style: ButtonStyle::Secondary,
        url: None,
        sku_id: None,
    });
    let next = Component::Button(Button {
        id: None,
        custom_id: Some(format!("docs:next:{page}")),
        disabled: page + 1 >= total,
        emoji: None,
        label: Some("Next >".to_owned()),
        style: ButtonStyle::Secondary,
        url: None,
        sku_id: None,
    });
    vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![prev, next],
    })]
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_limit() {
        let md = "a\n".repeat(5000);
        let chunks = chunk_markdown(&md, 2000);
        assert!(chunks.len() >= 3);
        for c in &chunks {
            assert!(c.chars().count() <= 2000);
        }
    }

    #[test]
    fn pagination_first_page() {
        let row = pagination_row(0, 3);
        assert_eq!(row.len(), 1);
    }
}
