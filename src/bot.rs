use anyhow::Result;
use handler::{Command, Handler};
use std::{env, sync::Arc};
use teloxide::{dispatching::UpdateHandler, prelude::*, update_listeners::webhooks};

mod handler;
mod settings;

#[derive(Clone)]
struct Config {
    source_channel: ChatId,
}

pub async fn run() -> Result<()> {
    let token = env::var("TG_TOKEN").expect("Telegram token not found");
    let bot = Bot::new(token);

    let bot_url = env::var("BOT_URL")
        .expect("BOT_URL not set")
        .parse()
        .expect("BOT_URL is in incorrect format");

    let bot_port = env::var("BOT_PORT")
        .expect("BOT_PORT not set")
        .parse::<u16>()
        .expect("BOT_PORT is not a number");

    let listener = webhooks::axum(
        bot.clone(),
        webhooks::Options::new(([0, 0, 0, 0], bot_port).into(), bot_url),
    )
    .await
    .expect("Webhook creation failed");

    let channel_id = env::var("CHANNEL_ID")
        .expect("CHANNEL_ID not set")
        .parse::<i64>()
        .expect("CHANNEL_ID is not a number");
    let config = Config {
        source_channel: ChatId(channel_id),
    };

    let settings_file = "/data/tforward_settings.json";

    let message_handler = Arc::new(Handler::new(settings_file));
    let command_handler = message_handler.clone();

    let handler: UpdateHandler<anyhow::Error> = dptree::entry()
        .branch(
            Update::filter_message().branch(
                dptree::entry()
                    .filter_command::<Command>()
                    .filter(|cfg: Config, msg: Message| msg.chat.id != cfg.source_channel)
                    .endpoint(move |_: Config, msg: Message, bot: Bot, cmd: Command| {
                        let command_handler = command_handler.clone();

                        async move {
                            command_handler.handle_command(&bot, &msg, &cmd).await?;
                            Ok(())
                        }
                    }),
            ),
        )
        .branch(Update::filter_channel_post().branch(
            dptree::filter(|cfg: Config, msg: Message| msg.chat.id == cfg.source_channel).endpoint(
                move |msg: Message, bot: Bot| {
                    let message_handler = message_handler.clone();

                    async move {
                        message_handler.handle_message(&bot, &msg).await?;
                        Ok(())
                    }
                },
            ),
        ));

    Box::pin(
        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![config])
            .default_handler(|upd| async move {
                tracing::warn!("Unhandled update: {:?}", upd);
            })
            .error_handler(LoggingErrorHandler::with_custom_text(
                "An error has occurred in the dispatcher",
            ))
            .enable_ctrlc_handler()
            .build()
            .dispatch_with_listener(
                listener,
                LoggingErrorHandler::with_custom_text("Listener failed"),
            ),
    )
    .await;

    Ok(())
}
