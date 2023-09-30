use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::ImageFormat;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{
    AttachmentType, ButtonStyle, CreateActionRow, CreateButton, Message, MessageId, ReactionType,
};
use poise::{CreateReply, PrefixFrameworkOptions};
use serenity::GatewayIntents;
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

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

fn add_delete_buttons(builder: &mut CreateReply) {
    builder.components(|b| {
        let mut action_row = CreateActionRow::default();
        let mut button = CreateButton::default();
        button
            .label("Delete")
            .style(ButtonStyle::Danger)
            .emoji(ReactionType::Unicode("🗑️".to_string()))
            .custom_id("delete");
        action_row.add_button(button);
        b.add_action_row(action_row)
    });
}

async fn enable_delete(ctx: Context<'_>, reply: Cow<'_, Message>) -> Result<(), Error> {
    let interaction = reply
        .await_component_interaction(ctx)
        .author_id(ctx.author().id)
        .await;

    if let Some(interaction) = interaction {
        if interaction.data.custom_id == "delete" {
            reply.delete(ctx).await?;
            // Some cleanup, but not really needed
            ctx.data().tex_cache.lock().await.remove(&reply.id);
        }
    }
    Ok(())
}

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

    let reply = if full_response.unwrap_or(false) || ctx.invoked_command_name() == "wa" {
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
            add_delete_buttons(b);
            b.reply(true)
        })
        .await?
    } else {
        let result = ctx.data().wolfram_alpha.short_answer(&query).await?;
        ctx.send(|b| {
            add_delete_buttons(b);
            b.reply(true)
                .embed(|e| e.title("Wolfram Alpha's result").description(result))
        })
        .await?
    };

    enable_delete(ctx, reply.message().await?).await?;

    Ok(())
}

#[poise::command(context_menu_command = "Render LaTeX")]
async fn tex_context_menu(ctx: Context<'_>, message: Message) -> Result<(), Error> {
    if let Some(id) = ctx.data().tex_cache.lock().await.get(&message.id) {
        // try to delete, if it is already gone that's fine too
        let _ = ctx.http().delete_message(message.channel_id.0, id.0).await;
    }

    ctx.defer().await?;
    let latex = message.content;
    let image = spawn_blocking(move || latex::render_to_png(&latex)).await?;

    if let Err(error) = &image {
        let res = ctx
            .send(|b| {
                add_delete_buttons(b);
                b.embed(|e| {
                    e.title("Error rendering LaTeX")
                        .description(error.to_string())
                })
            })
            .await?;
        let res = update_tex_cache(message.id, &res, ctx).await?;
        enable_delete(ctx, res).await?;
        return Ok(());
    }
    let image = image.unwrap();

    let res = ctx
        .send(|b| {
            add_delete_buttons(b);
            b.attachment(AttachmentType::Bytes {
                data: image.into(),
                filename: "latex.png".to_string(),
            })
        })
        .await?;
    let res = update_tex_cache(message.id, &res, ctx).await?;
    enable_delete(ctx, res).await?;

    Ok(())
}

async fn update_tex_cache<'a>(
    message_id: MessageId,
    reply_handle: &'a poise::ReplyHandle<'a>,
    ctx: Context<'a>,
) -> Result<Cow<'a, Message>, poise::serenity_prelude::Error> {
    let msg = reply_handle.message().await;
    if let Ok(msg) = &msg {
        ctx.data().tex_cache.lock().await.insert(message_id, msg.id);
    }
    msg
}

pub async fn start_bot(bot_context: BotContext) -> anyhow::Result<()> {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![wolfram(), register(), tex_context_menu()],
            prefix_options: PrefixFrameworkOptions {
                edit_tracker: Some(poise::EditTracker::for_timespan(
                    std::time::Duration::from_secs(600),
                )),
                case_insensitive_commands: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(GatewayIntents::non_privileged())
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(bot_context) }));

    Ok(framework.run().await?)
}
