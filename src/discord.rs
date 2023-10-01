use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use image::ImageFormat;
use poise::serenity_prelude::{
    AttachmentType, ButtonStyle, CreateActionRow, CreateButton, Member, Message,
    MessageComponentInteraction, MessageId, ReactionType, User, UserId,
};
use poise::{serenity_prelude as serenity, Event};
use poise::{CreateReply, PrefixFrameworkOptions};
use serenity::GatewayIntents;
use tokio::sync::Mutex;
use tokio::task::spawn_blocking;

use crate::latex;
use crate::wolframalpha::{WolframAlpha, WolframAlphaSimpleResult};

const DELETE_CUSTOM_ID: &str = "delete";

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

fn add_delete_buttons(owner: UserId, builder: &mut CreateReply) {
    builder.components(|b| {
        let mut action_row = CreateActionRow::default();
        let mut button = CreateButton::default();
        button
            .label("Delete")
            .style(ButtonStyle::Danger)
            .emoji(ReactionType::Unicode("🗑️".to_string()))
            .custom_id(format!("{DELETE_CUSTOM_ID}{}", owner.0));
        action_row.add_button(button);
        b.add_action_row(action_row)
    });
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

    if full_response.unwrap_or(false) || ctx.invoked_command_name() == "wa" {
        let result = ctx.data().wolfram_alpha.simple_query(&query).await?;
        let images = WolframAlphaSimpleResult::group_images(result.slice_image()?, 400);
        ctx.send(|b| {
            // Max is 10 but be nice
            images.iter().take(6).enumerate().for_each(|(index, img)| {
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
        .await?
    } else {
        let result = ctx.data().wolfram_alpha.short_answer(&query).await?;
        ctx.send(|b| {
            b.reply(true)
                .embed(|e| e.title("Wolfram Alpha's result").description(result))
        })
        .await?
    };

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
                b.embed(|e| {
                    e.title("Error rendering LaTeX")
                        .title("You can edit your message and try again.")
                        .description(error.to_string())
                })
            })
            .await?;
        update_tex_cache(message.id, &res, ctx).await;
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
    update_tex_cache(message.id, &res, ctx).await;

    Ok(())
}

async fn update_tex_cache<'a>(
    message_id: MessageId,
    reply_handle: &'a poise::ReplyHandle<'a>,
    ctx: Context<'a>,
) {
    if let Ok(msg) = reply_handle.message().await {
        ctx.data().tex_cache.lock().await.insert(message_id, msg.id);
    }
}

async fn handle_event<'a>(
    ctx: &'a serenity::Context,
    event: &'a Event<'a>,
    _framework: poise::FrameworkContext<'a, BotContext, Error>,
    _data: &'a BotContext,
) -> Result<(), Error> {
    if let Event::InteractionCreate { interaction } = event {
        if let Some(cmd) = interaction.as_message_component() {
            if let Some(member) = &cmd.member {
                if cmd.data.custom_id.starts_with(DELETE_CUSTOM_ID) {
                    handle_button_click(ctx, cmd, member).await?;
                }
            }
        }
    };
    Ok(())
}

async fn handle_button_click<'a>(
    ctx: &'a serenity::Context,
    cmd: &'a MessageComponentInteraction,
    member: &'a Member,
) -> Result<(), Error> {
    let author_id: u64 = cmd
        .data
        .custom_id
        .strip_prefix(DELETE_CUSTOM_ID)
        .unwrap()
        .parse()
        .unwrap();
    if author_id == member.user.id.0 {
        cmd.message.delete(ctx).await?;
    } else {
        cmd.create_interaction_response(ctx, |b| {
            b.interaction_response_data(|b| {
                b.ephemeral(true).embed(|e| {
                    e.title("You clicked a button.")
                        .description(interaction_unauthorized_message(&cmd.user))
                })
            })
        })
        .await?;
    }
    Ok(())
}

fn interaction_unauthorized_message(user: &User) -> &'static str {
    if user.id.0 == 140579104222085121 {
        "Bad bean, this isn't yours to click!"
    } else {
        "Good job. But this output was not generated for you, you cannot delete it."
    }
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
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },
            reply_callback: Some(|ctx, reply| {
                if reply.components.is_none() {
                    add_delete_buttons(ctx.author().id, reply);
                }
            }),
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"))
        .intents(GatewayIntents::non_privileged())
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(bot_context) }));

    let framework = framework.build().await?;
    let shard_manager = framework.shard_manager().clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    Ok(framework.start().await?)
}
