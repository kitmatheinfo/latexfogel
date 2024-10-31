use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use image::ImageFormat;
use log::{info, trace, warn};
use poise::serenity_prelude::{
    self as serenity, ButtonStyle, ComponentInteraction, CreateActionRow, CreateAttachment,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditAttachments, EditMessage, FullEvent, GatewayIntents, Member, Message, MessageId,
    ReactionType, User, UserId,
};
use poise::{CreateReply, EditTracker, PrefixFrameworkOptions};
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;

use crate::wolframalpha::{WolframAlpha, WolframAlphaSimpleResult};
use crate::{latex, ImageWidth};

const DELETE_CUSTOM_ID: &str = "delete";
const WIDEN_CUSTOM_ID: &str = "widen";

#[derive(Debug, Clone, Eq, PartialEq)]
struct WidenInfo {
    /// Owner of the original message.
    owner: UserId,
    /// LaTeX code used to generate the original response.
    latex: String,
}

pub struct BotContext {
    wolfram_alpha: WolframAlpha,

    /// Maps from message (with math) to our response (usually with image).
    rendered_cache: Arc<Mutex<HashMap<MessageId, MessageId>>>,

    /// Maps from our response (usually with image) to widening information.
    /// This info is only present if the image can be widened.
    widen_cache: Arc<Mutex<HashMap<MessageId, WidenInfo>>>,

    renderer_image: String,
}

impl BotContext {
    async fn rendered_response_id(&self, message_id: MessageId) -> Option<MessageId> {
        self.rendered_cache.lock().await.get(&message_id).copied()
    }

    async fn register_rendered_response_id(&self, message_id: MessageId, response_id: MessageId) {
        self.rendered_cache
            .lock()
            .await
            .insert(message_id, response_id);
    }

    async fn widen_info(&self, message_id: MessageId) -> Option<WidenInfo> {
        self.widen_cache.lock().await.get(&message_id).cloned()
    }

    async fn register_widen_info(&self, message_id: MessageId, info: WidenInfo) {
        self.widen_cache.lock().await.insert(message_id, info);
    }
}

impl BotContext {
    pub fn new(wolfram_alpha: WolframAlpha, renderer_image: String) -> Self {
        Self {
            wolfram_alpha,
            rendered_cache: Arc::new(Mutex::new(HashMap::new())),
            widen_cache: Arc::new(Mutex::new(HashMap::new())),
            renderer_image,
        }
    }
}

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, BotContext, Error>;

fn button_delete(owner: UserId) -> CreateButton {
    CreateButton::new(format!("{DELETE_CUSTOM_ID}{}", owner.get()))
        .label("Delete")
        .style(ButtonStyle::Danger)
        .emoji(ReactionType::Unicode("🗑️".to_string()))
}

fn button_wider(owner: UserId) -> CreateButton {
    CreateButton::new(format!("{WIDEN_CUSTOM_ID}{}", owner.get()))
        .label("Expand")
        .style(ButtonStyle::Primary)
        .emoji(ReactionType::Unicode("↔️".to_string()))
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
        ctx.send({
            let mut reply = CreateReply::default().reply(true);

            // Max is 10 but be nice
            for (idx, img) in images.iter().take(6).enumerate() {
                let mut buffer = Vec::new();
                img.write_to(&mut Cursor::new(&mut buffer), ImageFormat::Png)
                    .expect("write to buffer succeeds");

                reply = reply.attachment(CreateAttachment::bytes(buffer, format!("wa{idx}.png")));
            }

            reply
        })
        .await?
    } else {
        let result = ctx.data().wolfram_alpha.short_answer(&query).await?;
        ctx.send(
            CreateReply::default().reply(true).embed(
                CreateEmbed::default()
                    .title("Wolfram Alpha's result")
                    .description(result),
            ),
        )
        .await?
    };

    Ok(())
}

#[poise::command(context_menu_command = "Render LaTeX")]
async fn tex_context_menu(ctx: Context<'_>, message: Message) -> Result<(), Error> {
    if let Some(response_id) = ctx.data().rendered_response_id(message.id).await {
        // try to delete, if it is already gone that's fine too
        let _ = ctx
            .http()
            .delete_message(message.channel_id, response_id, None)
            .await;
    }

    ctx.defer().await?;

    let image = latex::render_latex(
        ctx.id(),
        ctx.data().renderer_image.clone(),
        message.content.clone(),
        ImageWidth::Normal,
    )
    .await;

    let image = match image {
        Ok(image) => image,
        Err(error) => {
            let handle = ctx
                .send(
                    CreateReply::default().embed(
                        CreateEmbed::default()
                            .title("Error rendering LaTeX")
                            .title("You can edit your message and try again.")
                            .description(error.to_string()),
                    ),
                )
                .await?;

            let response = handle.message().await?;

            ctx.data()
                .register_rendered_response_id(message.id, response.id)
                .await;

            return Ok(());
        }
    };

    let handle = ctx
        .send({
            let mut reply =
                CreateReply::default().attachment(CreateAttachment::bytes(image.png, "latex.png"));

            if image.overrun_hbox {
                reply = reply.components(vec![CreateActionRow::Buttons(vec![
                    button_delete(ctx.author().id),
                    button_wider(ctx.author().id),
                ])]);
            }

            reply
        })
        .await?;

    let response = handle.message().await?;

    ctx.data()
        .register_rendered_response_id(message.id, response.id)
        .await;

    if image.overrun_hbox {
        let info = WidenInfo {
            owner: ctx.author().id,
            latex: message.content,
        };
        ctx.data().register_widen_info(message.id, info).await;
    }

    Ok(())
}

#[poise::command(context_menu_command = "Render typst")]
async fn typst_context_menu(ctx: Context<'_>, message: Message) -> Result<(), Error> {
    if let Some(response_id) = ctx.data().rendered_response_id(message.id).await {
        // try to delete, if it is already gone that's fine too
        let _ = ctx
            .http()
            .delete_message(message.channel_id, response_id, None)
            .await;
    }

    ctx.defer().await?;

    let image = crate::typst::render_typst(
        ctx.id(),
        ctx.data().renderer_image.clone(),
        message.content.clone(),
    )
    .await;

    let image = match image {
        Ok(image) => image,
        Err(error) => {
            let handle = ctx
                .send(
                    CreateReply::default().embed(
                        CreateEmbed::default()
                            .title("Error rendering typst")
                            .title("You can edit your message and try again.")
                            .description(error.to_string()),
                    ),
                )
                .await?;

            let response = handle.message().await?;

            ctx.data()
                .register_rendered_response_id(message.id, response.id)
                .await;

            return Ok(());
        }
    };

    let handle = ctx
        .send(CreateReply::default().attachment(CreateAttachment::bytes(image.png, "typst.png")))
        .await?;

    let response = handle.message().await?;

    ctx.data()
        .register_rendered_response_id(message.id, response.id)
        .await;

    Ok(())
}

async fn handle_event<'a>(
    ctx: &'a serenity::Context,
    event: &'a FullEvent,
    _framework: poise::FrameworkContext<'a, BotContext, Error>,
    data: &'a BotContext,
) -> Result<(), Error> {
    if let FullEvent::InteractionCreate { interaction } = event {
        if let Some(cmd) = interaction.as_message_component() {
            trace!("Got interaction from '{}' ({})", cmd.user.name, cmd.user.id);
            if let Some(member) = &cmd.member {
                if cmd.data.custom_id.starts_with(DELETE_CUSTOM_ID) {
                    handle_delete_button_click(ctx, cmd, member).await?;
                } else if cmd.data.custom_id.starts_with(WIDEN_CUSTOM_ID) {
                    handle_widen_button_click(ctx, cmd, data).await?;
                }
            }
        }
    };
    Ok(())
}

async fn handle_widen_button_click<'a>(
    ctx: &'a serenity::Context,
    cmd: &'a ComponentInteraction,
    data: &'a BotContext,
) -> Result<(), Error> {
    let Some(info) = data.widen_info(cmd.message.id).await else {
        answer_unknown_button(ctx, cmd).await?;
        return Ok(());
    };

    if info.owner != cmd.user.id {
        return answer_action_not_allowed(ctx, cmd).await;
    }

    info!("Expanding for '{}' ({})", cmd.user.name, cmd.user.id);

    cmd.defer(ctx).await?;

    // Should work as we re-use the LaTeX
    let image = latex::render_latex(
        cmd.id.get(),
        data.renderer_image.clone(),
        info.latex,
        ImageWidth::Wide,
    )
    .await
    .unwrap();

    cmd.get_response(ctx)
        .await?
        .edit(
            ctx,
            EditMessage::default()
                .components(vec![CreateActionRow::Buttons(vec![button_delete(
                    cmd.user.id,
                )])])
                .attachments(
                    // Since we don't use EditAttachments::keep_all, all previous attachments are deleted.
                    EditAttachments::default().add(CreateAttachment::bytes(image.png, "latex.png")),
                ),
        )
        .await?;

    Ok(())
}

async fn answer_unknown_button<'a>(
    ctx: &'a serenity::Context,
    cmd: &'a ComponentInteraction,
) -> Result<(), Error> {
    cmd.create_response(
        ctx,
        // Maybe use Modal instead?
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::default()
                .ephemeral(true)
                .embed(
                    CreateEmbed::default()
                        .title("I don't remember that button. Was it before a restart?"),
                ),
        ),
    )
    .await?;

    Ok(())
}

async fn handle_delete_button_click<'a>(
    ctx: &'a serenity::Context,
    cmd: &'a ComponentInteraction,
    member: &'a Member,
) -> Result<(), Error> {
    let author_id: u64 = cmd
        .data
        .custom_id
        .strip_prefix(DELETE_CUSTOM_ID)
        .unwrap()
        .parse()
        .unwrap();
    if author_id == member.user.id.get() {
        cmd.message.delete(ctx).await?;
    } else {
        answer_action_not_allowed(ctx, cmd).await?;
    }
    Ok(())
}

async fn answer_action_not_allowed<'a>(
    ctx: &'a serenity::Context,
    cmd: &'a ComponentInteraction,
) -> Result<(), Error> {
    cmd.create_response(
        ctx,
        // Maybe use Modal instead?
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::default()
                .ephemeral(true)
                .embed(
                    CreateEmbed::default()
                        .title("You clicked a button.")
                        .description(interaction_unauthorized_message(&cmd.user)),
                ),
        ),
    )
    .await?;

    Ok(())
}

fn interaction_unauthorized_message(user: &User) -> &'static str {
    if user.id.get() == 140579104222085121 {
        "Bad bean, this isn't yours to click!"
    } else {
        "Good job. But this output was not generated for you, you cannot modify it."
    }
}

pub async fn start_bot(bot_context: BotContext) -> anyhow::Result<()> {
    let token = std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN");
    let intents = GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                wolfram(),
                register(),
                tex_context_menu(),
                typst_context_menu(),
            ],
            prefix_options: PrefixFrameworkOptions {
                edit_tracker: Some(Arc::new(EditTracker::for_timespan(Duration::from_secs(
                    600,
                )))),
                case_insensitive_commands: true,
                ..Default::default()
            },
            event_handler: |ctx, event, framework, data| {
                Box::pin(handle_event(ctx, event, framework, data))
            },
            reply_callback: Some(|ctx, reply| {
                if reply.components.is_none() {
                    reply.components(vec![CreateActionRow::Buttons(vec![button_delete(
                        ctx.author().id,
                    )])])
                } else {
                    reply
                }
            }),
            pre_command: |ctx| {
                Box::pin(async move {
                    info!(
                        "Executing command {}... for '{}' ({})",
                        ctx.command().name,
                        ctx.author().name,
                        ctx.author().id
                    );
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    info!(
                        "Executed command {} for '{}' ({})!",
                        ctx.command().name,
                        ctx.author().name,
                        ctx.author().id
                    );
                })
            },
            ..Default::default()
        })
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(bot_context) }))
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        let interrupt = tokio::signal::ctrl_c();
        select! {
            _ = sigterm.recv() => warn!("Received SIGTERM"),
            _ = interrupt => warn!("Received SIGINT")
        }
        shard_manager.shutdown_all().await;
    });

    Ok(client.start().await?)
}
