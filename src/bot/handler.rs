use anyhow::Result;
use teloxide::{
    macros::BotCommands,
    prelude::*,
    types::{Message, ReplyParameters},
    Bot,
};
use tokio::task::JoinSet;
use tracing::instrument;

use super::settings::Accessor;

#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported"
)]
pub enum Command {
    #[command(description = "subscribe to new messages")]
    Subscribe,
}

pub struct Handler {
    settings_accessor: Accessor,
}

impl Handler {
    pub fn new(settings_file: &str) -> Self {
        let settings_accessor = Accessor::new(settings_file);
        Self { settings_accessor }
    }

    #[instrument(skip(self, bot, msg), fields(channel_id = %msg.chat.id))]
    pub async fn handle_message(&self, bot: &Bot, msg: &Message) -> Result<()> {
        let recepients = self.settings_accessor.get_settings().await?.recepients;

        let mut join_set = JoinSet::new();

        for id in recepients.into_iter().map(ChatId) {
            tracing::info!("forwarding message to {recepient_id}", recepient_id = id);
            let bot = bot.clone();
            let msg = msg.clone();
            join_set.spawn(async move { bot.forward_message(id, msg.chat.id, msg.id).await });
        }

        while let Some(Ok(send_result)) = join_set.join_next().await {
            match send_result {
                Ok(msg) => {
                    tracing::info!(
                        "forwarded message to {recepient_id}",
                        recepient_id = msg.chat.id
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        "error while forwarding message. error: {error}",
                        error = err
                    );
                }
            }
        }

        Ok(())
    }

    #[instrument(skip(self, bot, msg), fields(chat_id = %msg.chat.id))]
    pub async fn handle_command(&self, bot: &Bot, msg: &Message, cmd: &Command) -> Result<()> {
        match cmd {
            Command::Subscribe => {
                self.settings_accessor.add_recepient(msg.chat.id.0).await?;
                bot.send_message(msg.chat.id, "Subscribed!")
                    .reply_parameters(ReplyParameters::new(msg.id))
                    .await?;
            }
        }

        Ok(())
    }
}
