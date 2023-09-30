use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::ImageFormat;
use poise::serenity_prelude::{AttachmentType, MessageId};
use poise::PrefixFrameworkOptions;
use poise::{serenity_prelude as serenity, ReplyHandle};
use serenity::GatewayIntents;
use tokio::sync::Mutex;

use crate::latex;
use crate::wolframalpha::{WolframAlpha, WolframAlphaSimpleResult};

pub struct BotContext {
    pub wolfram_alpha: WolframAlpha,
    tex_cache: Arc<Mutex<HashMap<MessageId, MessageId>>>,
}

impl BotContext {
    pub fn new(wolfram_alpha: WolframAlpha) -> Self {
        Self {
            wolfram_alpha,
            tex_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
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
    #[description = "Query"]
    #[rest]
    query: String,
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
            b.reply(true)
        })
        .await?;
    } else {
        let result = ctx.data().wolfram_alpha.short_answer(&query).await?;
        ctx.send(|b| {
            b.reply(true)
                .embed(|e| e.title("Wolfram Alpha's result").description(result))
        })
        .await?;
    }

    Ok(())
}

#[poise::command(
    context_menu_command = "Render LaTeX",
    slash_command,
    prefix_command,
    track_edits
)]
async fn tex(
    ctx: Context<'_>,
    #[description = "message"]
    #[rest]
    message: serenity::Message,
) -> Result<(), Error> {
    if let Some(id) = ctx.data().tex_cache.lock().await.get(&message.id) {
        ctx.http()
            .delete_message(message.channel_id.0, id.0)
            .await?;
    }

    ctx.defer().await?;
    let image = latex::render_to_png(&message.content);

    if let Err(error) = &image {
        let res = ctx
            .send(|b| {
                b.embed(|e| {
                    e.title("Error rendering LaTeX")
                        .description(error.to_string())
                })
            })
            .await?;
        update_tex_cache(message.id, res, ctx).await;
        return Ok(());
    }
    let image = image.unwrap();

    let res = ctx
        .send(|b| {
            b.attachment(AttachmentType::Bytes {
                data: image.into(),
                filename: "latex.png".to_string(),
            })
        })
        .await?;
    update_tex_cache(message.id, res, ctx).await;

    Ok(())
}

async fn update_tex_cache(message_id: MessageId, reply_handle: ReplyHandle<'_>, ctx: Context<'_>) {
    if let Ok(msg) = reply_handle.message().await {
        ctx.data().tex_cache.lock().await.insert(message_id, msg.id);
    }
}

pub async fn start_bot(bot_context: BotContext) -> anyhow::Result<()> {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![wolfram(), register(), tex()],
            prefix_options: PrefixFrameworkOptions {
                prefix: Some("=".to_string()),
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
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(bot_context) }));

    Ok(framework.run().await?)
}
