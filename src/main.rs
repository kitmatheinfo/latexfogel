use crate::discord::BotContext;
use crate::wolframalpha::WolframAlpha;

mod discord;
mod latex;
mod pdf;
mod wolframalpha;

#[tokio::main]
async fn main() {
    discord::start_bot(BotContext::new(WolframAlpha::new(
        std::env::var("WOLFRAM_TOKEN").expect("missing WOLFRAM_TOKEN"),
    )))
    .await
    .expect("Error during bot startup");
}
