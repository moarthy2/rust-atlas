---
name: reference-skill
description: Authoritative Rust Reference sections (syntax, types, lifetimes, keywords, attributes, grammar). Use for precise language-semantics questions.
---

# Reference Skill

- Source: https://doc.rust-lang.org/reference/print.html
- Strategy: fetch print.html once (cached 24h), split on h1-h4 via `tl`,
  fuzzy-match headings with `nucleo-matcher`, substring fallback on body.
- Output: `## <title> — <heading>` + canonical link + ≤1800 chars body,
  chunked to Discord embeds.
- Trigger keywords: grammar, syntax, keyword, lifetime, ABI, attribute, pattern, expression, statement, item, unsized, reference, spec.
