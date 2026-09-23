//! Discord bot entry: gateway shard + command registration.

pub mod commands;
pub mod handler;
pub mod render;
pub mod state;

use tracing::{error, info, warn};
use twilight_gateway::{
    Event, EventTypeFlags, Intents, Shard, ShardId, ShardState, StreamExt as _,
};

use self::handler::handle_interaction;
use self::state::AppState;

/// Run the bot until Ctrl-C. Only subscribes to GUILDS + INTERACTION_CREATE
/// (minimal intents = minimal memory/CPU).
pub async fn run(state: AppState) -> anyhow::Result<()> {
    register_commands(&state).await;

    if state.config.preload_docs {
        info!("preloading docs...");
        state.store.preload().await;
    }

    let mut shard = Shard::new(
        ShardId::ONE,
        state.config.discord_token.clone(),
        Intents::GUILDS,
    );

    info!("shard connecting...");
    loop {
        let item = shard.next_event(EventTypeFlags::INTERACTION_CREATE).await;
        let Some(item) = item else { break };
        match item {
            // InteractionCreate is Box<InteractionCreate> which derefs to Interaction.
            Ok(Event::InteractionCreate(interaction)) => {
                let state = state.clone();
                let inner: Box<twilight_model::application::interaction::Interaction> =
                    Box::new(interaction.0.clone());
                tokio::spawn(async move {
                    handle_interaction(inner, state).await;
                });
            }
            Ok(_) => {}
            Err(source) => {
                warn!(?source, "gateway error");
                if matches!(shard.state(), ShardState::FatallyClosed) {
                    error!("shard fatally closed");
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn register_commands(state: &AppState) {
    let commands = self::commands::global_commands();
    // Need application id: prefer env, else fetch current application info.
    let app_id = match state.config.application_id {
        Some(id) => twilight_model::id::Id::new(id),
        None => match state.discord.current_user_application().await {
            Ok(resp) => match resp.model().await {
                Ok(app) => app.id,
                Err(e) => {
                    warn!(error = %e, "cannot parse application info; skipping command registration");
                    return;
                }
            },
            Err(e) => {
                warn!(error = %e, "cannot fetch application id; skipping command registration (set APPLICATION_ID)");
                return;
            }
        },
    };
    match state
        .discord
        .interaction(app_id)
        .set_global_commands(&commands)
        .await
    {
        Ok(_) => info!("registered {} global commands", commands.len()),
        Err(e) => warn!(error = %e, "command registration failed"),
    }
}
