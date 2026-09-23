//! Interaction handlers: slash commands + pagination buttons (part 1).

use tracing::{info, warn};
use twilight_model::{
    application::interaction::{Interaction, InteractionData, InteractionType},
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};

use super::{commands::string_option, render::chunk_markdown, state::AppState};
use crate::docs::DocSource;

/// Route one InteractionCreate event (spawned per event).
pub async fn handle_interaction(interaction: Box<Interaction>, state: AppState) {
    if interaction.kind != InteractionType::ApplicationCommand
        && interaction.kind != InteractionType::MessageComponent
    {
        return;
    }
    match interaction.data.clone() {
        Some(InteractionData::ApplicationCommand(data)) => {
            handle_command(*interaction, data.name.as_str(), &data.options, state).await;
        }
        Some(InteractionData::MessageComponent(data)) => {
            handle_component(*interaction, &data.custom_id, state).await;
        }
        _ => {}
    }
}

async fn handle_command(
    interaction: Interaction,
    name: &str,
    options: &[twilight_model::application::interaction::application_command::CommandDataOption],
    state: AppState,
) {
    match name {
        "ping" => {
            respond(&state, &interaction, "Pong! Rust docs bot is alive.").await;
        }
        "help" => {
            respond(&state, &interaction, &help_text()).await;
        }
        "docs" => {
            let query = string_option(options, "query").unwrap_or_default();
            let source_raw = string_option(options, "source").unwrap_or_default();
            if query.trim().is_empty() {
                respond(&state, &interaction, "Please provide a search query.").await;
                return;
            }
            if !defer(&state, &interaction).await {
                return;
            }
            match run_docs(&state, &query, &source_raw).await {
                Ok((pages, title, color, footer, url)) => {
                    followup_pages(&state, &interaction, pages, title, color, footer, url).await;
                }
                Err(e) => {
                    warn!(error = %e, "docs failed");
                    followup_text(&state, &interaction, &format!(" {e}")).await;
                }
            }
        }
        "ask" => {
            let question = string_option(options, "question").unwrap_or_default();
            if question.trim().is_empty() {
                respond(&state, &interaction, "Please ask a question.").await;
                return;
            }
            if !defer(&state, &interaction).await {
                return;
            }
            let decision = state.ai.route(&state.skills, &question).await;
            info!(skill = %decision.skill_id, via_ai = decision.via_ai, "routed ask");
            let skill = state
                .skills
                .get(&decision.skill_id)
                .unwrap_or_else(|| state.skills.heuristic_route(&question));
            match skill.run(&decision.refined_query, &state.store).await {
                Ok(out) => {
                    let prefix = if decision.via_ai {
                        format!(
                            "AI routed to **{}** (refined: \"{}\")\n\n",
                            out.source.title(),
                            decision.refined_query
                        )
                    } else {
                        format!("Heuristic routed to **{}**\n\n", out.source.title())
                    };
                    let md = format!("{prefix}{}", out.markdown);
                    followup_pages(
                        &state,
                        &interaction,
                        chunk_markdown(&md, 4000),
                        out.title.clone(),
                        out.source.embed_color(),
                        out.source.title().to_owned(),
                        Some(out.source.canonical_url().to_owned()),
                    )
                    .await;
                }
                Err(e) => {
                    let hits = state.store.search_all(&decision.refined_query, 2).await;
                    if hits.is_empty() {
                        followup_text(&state, &interaction, &format!(" {e}")).await;
                    } else {
                        let mut md = format!(
                            "No direct hit in **{}**; closest:\n\n",
                            skill.source().title()
                        );
                        for h in hits.iter().take(3) {
                            md.push_str(&crate::skills::format_section_markdown(h));
                            md.push_str("\n---\n");
                        }
                        followup_pages(
                            &state,
                            &interaction,
                            chunk_markdown(&md, 4000),
                            "Closest matches".to_owned(),
                            0x5865F2,
                            "multi-source".to_owned(),
                            None,
                        )
                        .await;
                    }
                }
            }
        }
        "context7" => {
            let library = string_option(options, "library").unwrap_or_default();
            let query = string_option(options, "query").unwrap_or_default();
            if !defer(&state, &interaction).await {
                return;
            }
            if !state.context7.is_configured() {
                followup_text(&state, &interaction, "Context7 not configured. Set CONTEXT7_API_KEY (https://context7.com/dashboard).").await;
                return;
            }
            match state.context7.fetch(&library, &query).await {
                Ok(md) => {
                    let full = format!("**Context7 - {library}** — {query}\n\n{md}");
                    followup_pages(
                        &state,
                        &interaction,
                        chunk_markdown(&full, 4000),
                        format!("{library}: {query}"),
                        0x00BFFF,
                        "via Context7".to_owned(),
                        None,
                    )
                    .await;
                }
                Err(e) => {
                    followup_text(&state, &interaction, &format!(" Context7 error: {e}")).await;
                }
            }
        }
        _ => {}
    }
}

/// Core /docs flow: explicit source or AI-routed, with fallback.
async fn run_docs(
    state: &AppState,
    query: &str,
    source_raw: &str,
) -> anyhow::Result<(Vec<String>, String, u32, String, Option<String>)> {
    let auto = source_raw.trim().is_empty() || source_raw.eq_ignore_ascii_case("auto");
    let source = if auto {
        let d = state.ai.route(&state.skills, query).await;
        state
            .skills
            .get(&d.skill_id)
            .map(|s| s.source())
            .unwrap_or(DocSource::Book)
    } else if let Some(s) = DocSource::from_id(source_raw) {
        s
    } else {
        anyhow::bail!(
            "Unknown source \"{source_raw}\". Use auto, book, reference, by-example, nomicon."
        );
    };
    let skill = state.skills.get(source.id()).expect("skill exists");
    match skill.run(query, &state.store).await {
        Ok(out) => {
            let via = if auto {
                format!("{} - auto-routed", source.title())
            } else {
                source.title().to_owned()
            };
            Ok((
                chunk_markdown(&out.markdown, 4000),
                out.title,
                source.embed_color(),
                via,
                Some(source.canonical_url().to_owned()),
            ))
        }
        Err(_) => {
            let hits = state.store.search_all(query, 2).await;
            if hits.is_empty() {
                anyhow::bail!("No matches for \"{query}\". Try /ask or /context7.");
            }
            let mut md = String::from("Closest sections across all Rust docs:\n\n");
            for h in hits.iter().take(3) {
                md.push_str(&crate::skills::format_section_markdown(h));
                md.push_str("\n---\n");
            }
            Ok((
                chunk_markdown(&md, 4000),
                "Closest matches".to_owned(),
                0x5865F2,
                "multi-source".to_owned(),
                None,
            ))
        }
    }
}

async fn respond(state: &AppState, interaction: &Interaction, content: &str) {
    use twilight_model::channel::message::MessageFlags;
    let _ = MessageFlags::EPHEMERAL;
    let resp = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(content.to_owned()),
            ..Default::default()
        }),
    };
    if let Err(e) = state
        .discord
        .interaction(interaction.application_id)
        .create_response(interaction.id, &interaction.token, &resp)
        .await
    {
        warn!(error = %e, "create_response failed");
    }
}

async fn defer(state: &AppState, interaction: &Interaction) -> bool {
    let resp = InteractionResponse {
        kind: InteractionResponseType::DeferredChannelMessageWithSource,
        data: None,
    };
    state
        .discord
        .interaction(interaction.application_id)
        .create_response(interaction.id, &interaction.token, &resp)
        .await
        .map_err(|e| {
            warn!(error = %e, "defer failed");
            e
        })
        .is_ok()
}

async fn followup_text(state: &AppState, interaction: &Interaction, text: &str) {
    let chunks = chunk_markdown(text, 2000);
    let first = chunks.first().cloned().unwrap_or_default();
    if let Err(e) = state
        .discord
        .interaction(interaction.application_id)
        .create_followup(&interaction.token)
        .content(&first)
        .await
    {
        warn!(error = %e, "followup failed");
    }
}

async fn followup_pages(
    state: &AppState,
    interaction: &Interaction,
    pages: Vec<String>,
    title: String,
    color: u32,
    footer: String,
    url: Option<String>,
) {
    use super::render::{build_embed, pagination_row};
    use std::sync::Arc;
    if pages.is_empty() {
        followup_text(state, interaction, "No content.").await;
        return;
    }
    let total = pages.len();
    // Key pages by the followup message id so button presses (which carry
    // interaction.message.id) can find them. create_followup returns the
    // Message model; use its id.
    let footer_txt = format!(
        "{} - page 1/{total}",
        footer.chars().take(1500).collect::<String>()
    );
    let embed = build_embed(&title, &pages[0], color, &footer_txt, url.as_deref());
    let components = if total > 1 {
        Some(pagination_row(0, total))
    } else {
        None
    };
    let components_ref: Option<&[twilight_model::channel::message::Component]> =
        components.as_deref();
    let embeds_arr = [embed];
    // N.B. embeds()/components() borrow their args, and CreateFollowup
    // borrows the InteractionClient, so keep both alive until after `.await`.
    let client = state.discord.interaction(interaction.application_id);
    let req = client.create_followup(&interaction.token);
    let req = req.embeds(&embeds_arr);
    let req = match components_ref {
        Some(c) => req.components(c),
        None => req,
    };
    match req.await {
        Ok(resp) => {
            // Cache pages under message id for pagination buttons.
            match resp.model().await {
                Ok(msg) => {
                    state
                        .pages
                        .insert(msg.id.to_string(), Arc::new(pages))
                        .await;
                }
                Err(e) => {
                    warn!(error = %e, "followup model parse failed");
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "followup embed failed");
            followup_text(state, interaction, &pages[0]).await;
        }
    }
}

async fn handle_component(interaction: Interaction, custom_id: &str, state: AppState) {
    use super::render::{build_embed, pagination_row};
    // custom_id: docs:prev:<current_page> | docs:next:<current_page>
    // Pages lookup: component interactions carry `message.id` of the bot's
    // own followup message. We keyed pages by message id after sending the
    // followup (see followup_pages), so pagination is stateless w.r.t. tokens.
    let parts: Vec<&str> = custom_id.split(':').collect();
    if parts.len() != 3 || parts[0] != "docs" {
        return;
    }
    let dir = parts[1];
    let page: usize = parts[2].parse().unwrap_or(0);
    let target = if dir == "next" {
        page + 1
    } else {
        page.saturating_sub(1)
    };
    let msg_id = match &interaction.message {
        Some(m) => m.id.to_string(),
        None => {
            ack_component(
                &state,
                &interaction,
                "This pagination expired — run the command again.",
            )
            .await;
            return;
        }
    };
    let Some(pages) = state.pages.get(&msg_id).await else {
        ack_component(
            &state,
            &interaction,
            "This pagination expired — run the command again.",
        )
        .await;
        return;
    };
    if target >= pages.len() {
        return;
    }
    let total = pages.len();
    let embed = build_embed(
        "Rust docs",
        &pages[target],
        0xCE412B,
        &format!("page {}/{}", target + 1, total),
        None,
    );
    let update = InteractionResponse {
        kind: InteractionResponseType::UpdateMessage,
        data: Some(InteractionResponseData {
            embeds: Some(vec![embed]),
            components: Some(pagination_row(target, total)),
            ..Default::default()
        }),
    };
    if let Err(e) = state
        .discord
        .interaction(interaction.application_id)
        .create_response(interaction.id, &interaction.token, &update)
        .await
    {
        warn!(error = %e, "component update failed");
    }
}

async fn ack_component(state: &AppState, interaction: &Interaction, text: &str) {
    let resp = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(text.to_owned()),
            flags: Some(twilight_model::channel::message::MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    };
    let _ = state
        .discord
        .interaction(interaction.application_id)
        .create_response(interaction.id, &interaction.token, &resp)
        .await;
}

fn help_text() -> String {
    "Rust Docs Bot — fast, tiny, creative.\n\nCommands\n- /docs <query> [source] — search Reference / By Example / Nomicon / Book. source = auto, book, reference, by-example, nomicon.\n- /ask <question> — natural language; AI Router Skill picks the right docs Skill.\n- /context7 <library> <query> — fresh crate docs via Context7 (needs CONTEXT7_API_KEY).\n- /ping, /help\n\nTips: /docs ownership source:book, /ask how do I borrow across functions?, /context7 tokio spawn.".to_owned()
}
