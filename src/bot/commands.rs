//! Slash command definitions + option parsing.

use twilight_model::application::command::{
    Command, CommandOption, CommandOptionType, CommandType,
};
use twilight_model::id::{Id, marker::CommandVersionMarker};

fn opt(name: &str, desc: &str, required: bool, autocomplete: bool) -> CommandOption {
    CommandOption {
        autocomplete: autocomplete.then_some(true),
        channel_types: None,
        choices: None,
        description: desc.to_owned(),
        description_localizations: None,
        kind: CommandOptionType::String,
        max_length: None,
        max_value: None,
        min_length: None,
        min_value: None,
        name: name.to_owned(),
        name_localizations: None,
        options: None,
        required: required.then_some(true),
    }
}

#[allow(deprecated)]
fn cmd(name: &str, desc: &str, options: Vec<CommandOption>) -> Command {
    Command {
        application_id: None,
        contexts: None,
        default_member_permissions: None,
        #[allow(deprecated)]
        dm_permission: None,
        description: desc.to_owned(),
        description_localizations: None,
        guild_id: None,
        id: None,
        integration_types: None,
        kind: CommandType::ChatInput,
        name: name.to_owned(),
        name_localizations: None,
        nsfw: None,
        options,
        version: Id::<CommandVersionMarker>::new(1),
    }
}

/// Global commands registered on startup (idempotent via set_global_commands).
pub fn global_commands() -> Vec<Command> {
    vec![
        cmd(
            "docs",
            "Search Rust docs (Reference, By Example, Nomicon, Book)",
            vec![
                opt(
                    "query",
                    "What to look up, e.g. ownership or unsafe transmute",
                    true,
                    false,
                ),
                opt(
                    "source",
                    "Book, Reference, By Example, or Nomicon (default: auto)",
                    false,
                    false,
                ),
            ],
        ),
        cmd(
            "ask",
            "Ask in natural language — AI routes to the right docs Skill",
            vec![opt("question", "Your Rust question", true, false)],
        ),
        cmd(
            "context7",
            "Fresh library docs via Context7 (e.g. tokio, serenity)",
            vec![
                opt(
                    "library",
                    "Library name, e.g. tokio or serenity",
                    true,
                    false,
                ),
                opt("query", "What to look up", true, false),
            ],
        ),
        cmd("help", "Show help for the Rust docs bot", vec![]),
        cmd("ping", "Check the bot is alive", vec![]),
    ]
}

/// Extract a string option from slash-command options.
pub fn string_option(
    options: &[twilight_model::application::interaction::application_command::CommandDataOption],
    name: &str,
) -> Option<String> {
    use twilight_model::application::interaction::application_command::CommandOptionValue;
    options
        .iter()
        .find(|o| o.name == name)
        .and_then(|o| match &o.value {
            CommandOptionValue::String(s) => Some(s.clone()),
            _ => None,
        })
}
