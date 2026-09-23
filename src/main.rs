#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod ai;
mod bot;
mod config;
mod context7;
mod docs;
mod skills;

use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("rust_docs_bot=info".parse().unwrap()),
        )
        .init();

    let config = config::Config::from_env()?;
    let state = bot::state::AppState::new(config);
    bot::run(state).await
}
