use clap::{Parser, Subcommand};
use log::warn;

use crate::discord::BotContext;
use crate::wolframalpha::WolframAlpha;

mod discord;
mod latex;
mod pdf;
mod wolframalpha;

#[derive(Subcommand)]
enum Command {
    Bot { renderer_docker_image: String },
    Renderer,
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if !std::env::temp_dir().exists() {
        std::fs::create_dir_all(std::env::temp_dir()).unwrap();
        warn!("Created my temp dir: {:?}", std::env::temp_dir());
    }

    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", "latexfogel=info"));

    match args.command {
        Command::Bot {
            renderer_docker_image,
        } => start_bot(renderer_docker_image).await,
        Command::Renderer => latex::run_renderer().await,
    }
}

async fn start_bot(renderer_docker_image: String) {
    discord::start_bot(BotContext::new(
        WolframAlpha::new(std::env::var("WOLFRAM_TOKEN").expect("missing WOLFRAM_TOKEN")),
        renderer_docker_image,
    ))
    .await
    .expect("Error during bot startup");
}
