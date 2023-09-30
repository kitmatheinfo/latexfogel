use std::io::Cursor;

use image::ImageFormat;
use poise::serenity_prelude::{AttachmentType, GatewayIntents};
use poise::PrefixFrameworkOptions;

use crate::wolframalpha::{WolframAlpha, WolframAlphaSimpleResult};

pub struct BotContext {
    pub wolfram_alpha: WolframAlpha,
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotContext, Error>;

#[poise::command(prefix_command)]
pub async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, aliases("wa", "pup"))]
async fn wolfram(
    ctx: Context<'_>,
    #[description = "Show full response"] full_response: Option<bool>,
    #[description = "Query"] #[rest] query: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    if full_response.unwrap_or(false) || ctx.invoked_command_name() == "wa" {
        let result = ctx.data().wolfram_alpha.simple_query(&query).await?;
        let images = WolframAlphaSimpleResult::group_images(result.slice_image()?, 400);
        ctx.send(|b| {
            images.iter().enumerate().for_each(|(index, img)| {
                let mut buffer = Vec::new();
                img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
                    .expect("write to buffer succeeds");

                b.attachment(AttachmentType::Bytes {
                    data: buffer.into(),
                    filename: format!("wa{index}.png"),
                });
            });
            b
        })
        .await?;
    } else {
        let result = ctx.data().wolfram_alpha.short_answer(&query).await?;
        ctx.send(|b| b.embed(|e| e.title("Wolfram Alpha's result").description(result)))
            .await?;
    }

    Ok(())
}

pub async fn start_bot(bot_context: BotContext) -> anyhow::Result<()> {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![wolfram(), register()],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some("=".into()),
                edit_tracker: Some(poise::EditTracker::for_timespan(
                    std::time::Duration::from_secs(3600),
                )),
                case_insensitive_commands: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT)
        .setup(|_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(bot_context)
            })
        });

    Ok(framework.run().await?)
}
