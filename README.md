# Rust Atlas

A small Discord bot for finding the right Rust documentation without leaving the conversation. Rust Atlas searches the official Rust Book, Reference, Rust by Example, and Rustonomicon, then formats the result for Discord. An optional OpenAI-compatible router can choose the best source for a question, and Context7 can provide current crate documentation.

The bot is deliberately conservative about resources: documentation is fetched on demand, caches have fixed limits, and the gateway uses only the intents it needs.

## What it does

- `/docs` searches one documentation source or all four.
- `/ask` routes a natural-language question to the most useful Rust source.
- `/context7` looks up current documentation for a crate or library.
- `/ping` and `/help` provide basic checks and command guidance.
- Long results are split into paginated Discord embeds.
- If the AI endpoint is unavailable, local keyword routing keeps `/ask` usable.

## Requirements

- A Discord bot token.
- Rust 1.85 or newer for local builds.
- No privileged Discord intents are required.

## Run locally

```sh
git clone https://github.com/moarthy2/rust-atlas.git
cd rust-atlas
cp .env.example .env
# Edit .env and set DISCORD_TOKEN
cargo run --release
```

The bot registers its slash commands when it starts. `APPLICATION_ID` is optional; set it when you want command registration to use a specific application directly.

## Configuration

| Variable | Required | Purpose |
| --- | --- | --- |
| `DISCORD_TOKEN` | yes | Discord bot token |
| `APPLICATION_ID` | no | Discord application ID |
| `AI_BASE_URL` | no | OpenAI-compatible API base URL |
| `AI_API_KEY` | no | Key for the router endpoint |
| `AI_MODEL` | no | Router model name, default `gpt-4o-mini` |
| `CONTEXT7_API_KEY` | no | Enables `/context7` |
| `PRELOAD_DOCS` | no | Load all docs at startup when `true` |
| `RUST_LOG` | no | Log filter, default shown in `.env.example` |

Never commit `.env`. The example file contains placeholders only.

## Commands

- `/docs query:<text> source:auto`
- `/ask question:<text>`
- `/context7 library:<name> query:<text>`
- `/ping`
- `/help`

Available documentation sources are `auto`, `book`, `reference`, `by-example`, and `nomicon`.

## Design notes

The project uses Twilight instead of a full Discord framework, a single-thread Tokio runtime, bounded Moka caches, and lazy document loading. Release builds use size optimization, LTO, one codegen unit, symbol stripping, and an aborting panic strategy. These choices keep the idle footprint reasonable on a small VPS, a laptop, or a Termux device.

The documentation parser consumes the official `print.html` pages and keeps only indexed sections. The source URLs are defined in `src/docs.rs`, so a documentation site change can be fixed in one place.

## Build

```sh
cargo check --locked
cargo test --locked
cargo build --locked --release
```

GitHub Actions runs checks for every push and builds release archives for Linux x86_64, Linux ARM64, Windows x86_64, macOS x86_64, macOS ARM64, and Android/Termux ARM64. A version tag such as `v0.1.0` packages the binaries and publishes a GitHub Release.

Android artifacts are Linux ELF binaries linked for Android's ARM64 userspace. They are intended for Termux or another Android environment that can execute native binaries; they are not APKs.

## License

MIT
