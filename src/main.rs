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
    }
    discord::start_bot(BotContext::new(WolframAlpha::new(
        std::env::var("WOLFRAM_TOKEN").expect("missing WOLFRAM_TOKEN"),
    )))
    .await
    .expect("Error during bot startup");
}
