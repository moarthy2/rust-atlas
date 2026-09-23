---
name: router-skill
description: Routes a natural-language query to one of reference / by-example / nomicon / book via the custom AI provider endpoint, with heuristic fallback.
---

# Router Skill

- Endpoint: `{AI_BASE_URL}/chat/completions` (OpenAI-compatible), model `{AI_MODEL}`, key `AI_API_KEY`.
- System prompt carries the 4-skill catalogue; model replies ONLY `{"skill": "<id>", "query": "<refined 2-8 keywords>"}`.
- Lenient parse (strips code fences); unknown skill → heuristic re-route.
- No key / any error → `SkillRegistry::heuristic_route` keyword scores.
