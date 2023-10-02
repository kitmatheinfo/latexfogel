use std::env::args;

use log::{error, warn};

use crate::discord::BotContext;
use crate::wolframalpha::WolframAlpha;

mod discord;
mod latex;
mod pdf;
mod wolframalpha;

#[tokio::main]
async fn main() {
    if !std::env::temp_dir().exists() {
        std::fs::create_dir_all(std::env::temp_dir()).unwrap();
        warn!("Created my temp dir: {:?}", std::env::temp_dir());
    }

    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", "latexfogel=info"));

    if args().len() < 2 {
        print_usage();
        return;
    }
    let command = args().nth(1).unwrap();
    if command == "bot" {
        start_bot().await;
    } else if command == "renderer" {
        latex::run_renderer().await;
    } else {
        error!("Unknown command {command:?}");
    }
}

async fn start_bot() {
    if args().len() < 3 {
        error!("[renderer docker image] argument required");
        print_usage();
        return;
    }
    let renderer_docker_image = args().nth(2).unwrap();

    discord::start_bot(BotContext::new(
        WolframAlpha::new(std::env::var("WOLFRAM_TOKEN").expect("missing WOLFRAM_TOKEN")),
        renderer_docker_image,
    ))
    .await
    .expect("Error during bot startup");
}

fn print_usage() {
    error!(
        "Usage: {} <bot | renderer> [renderer docker image]",
        args().next().unwrap()
    );
}
