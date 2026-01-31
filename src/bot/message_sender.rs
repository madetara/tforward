use anyhow::Result;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{instrument, Level};

use teloxide::{
    payloads::ForwardMessagesSetters,
    prelude::Requester,
    types::{ChatId, MessageId},
    Bot,
};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        Mutex,
    },
    task::JoinSet,
    time::interval,
};

use super::settings::Accessor;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct MediaGroupId(pub String);

#[derive(Clone, Debug)]
struct MediaGroupInfo {
    from: ChatId,
    message_ids: Vec<MessageId>,
    last_message_timestamp: u64,
}

impl MediaGroupInfo {
    fn new(from: ChatId) -> Self {
        let message_ids = vec![];
        let last_message_timestamp = seconds_since_unix_epoch();
        Self {
            from,
            message_ids,
            last_message_timestamp,
        }
    }
}

pub struct MessageInfoReciever(pub UnboundedReceiver<MessageInfo>);

pub struct MessageInfoSender(pub UnboundedSender<MessageInfo>);

#[derive(Debug)]
pub struct MessageInfo {
    from: ChatId,
    id: MessageId,
    media_group_id: MediaGroupId,
}

pub struct MessageSender {
    reciever: MessageInfoReciever,
    settings: Arc<Accessor>,
    send_plan: Arc<Mutex<HashMap<MediaGroupId, Arc<Mutex<MediaGroupInfo>>>>>,
    bot: Bot,
}

const PAUSE_DURATION: Duration = Duration::from_secs(5);
const MESSAGE_SEND_DELAY_SECONDS: u64 = 10;

impl MessageSender {
    pub fn new(reciever: MessageInfoReciever, settings: Arc<Accessor>, bot: Bot) -> Self {
        let send_plan = Arc::new(Mutex::new(HashMap::new()));
        Self {
            reciever,
            settings,
            send_plan,
            bot,
        }
    }

    pub async fn run(mut self) {
        let mut ticker = interval(PAUSE_DURATION);

        loop {
            tokio::select! {
                maybe_message = self.reciever.0.recv() => {
                    let Some(message_info) = maybe_message else {
                        if let Err(error) = self.try_send_messages().await {
                            tracing::warn!("error occured while sending messages, details: {}", error);
                        }
                        break;
                    };

                    tracing::info!("adding message to plan: {:?}", message_info);

                    let mut send_plan_lock = self.send_plan.lock().await;
                    let entry = send_plan_lock
                        .entry(message_info.media_group_id)
                        .or_insert_with(|| {
                            Arc::new(Mutex::new(MediaGroupInfo::new(message_info.from)))
                        });

                    let mut entry_lock = entry.lock().await;
                    entry_lock.last_message_timestamp = seconds_since_unix_epoch();
                    entry_lock.message_ids.push(message_info.id);

                    drop(entry_lock);
                    drop(send_plan_lock);
                }
                _ = ticker.tick() => {
                    tracing::debug!("tick: trying to send");
                    if let Err(error) = self.try_send_messages().await {
                        tracing::warn!("error occured while sending messages, details: {}", error);
                    }
                }
            }
        }
    }

    #[instrument(skip(self))]
    async fn try_send_messages(&self) -> Result<()> {
        tracing::info!("attempting to send messages");

        let current_time = seconds_since_unix_epoch();
        let candidates = {
            let send_plan_lock = self.send_plan.lock().await;
            send_plan_lock
                .iter()
                .map(|(media_group_id, media_group_info)| {
                    (media_group_id.clone(), Arc::clone(media_group_info))
                })
                .collect::<Vec<_>>()
        };

        let mut ready_groups = Vec::new();
        for (media_group_id, media_group_info) in candidates {
            let media_group_info_lock = media_group_info.lock().await;
            if current_time
                .saturating_sub(media_group_info_lock.last_message_timestamp)
                < MESSAGE_SEND_DELAY_SECONDS
            {
                tracing::info!("not enough time has passed. skipping");
                continue;
            }

            tracing::info!("processing media group: {:?}", media_group_info);
            drop(media_group_info_lock);
            ready_groups.push((media_group_id, media_group_info));
        }

        if ready_groups.is_empty() {
            return Ok(());
        }

        {
            let mut send_plan_lock = self.send_plan.lock().await;
            for (media_group_id, _) in &ready_groups {
                send_plan_lock.remove(media_group_id);
            }
        }

        let recepients = self.settings.get_settings().await?.recepients;

        for (_, media_group_info) in ready_groups {
            let mut join_set = JoinSet::new();
            let (from, message_ids) = {
                let media_group_info = media_group_info.lock().await;
                let mut message_ids = media_group_info.message_ids.clone();
                message_ids.sort_by(|&a, &b| a.0.cmp(&b.0));
                (media_group_info.from, message_ids)
            };

            for recepient in &recepients {
                tracing::info!(
                    "forwarding message to {recepient_id}",
                    recepient_id = recepient.chat_id
                );
                let bot = self.bot.clone();
                let message_ids = message_ids.clone();
                let recepient = recepient.clone();

                join_set.spawn(async move {
                    let span = tracing::span!(
                        Level::INFO,
                        "forwarding message",
                        recepient = recepient.chat_id.0
                    );
                    let _enter = span.enter();

                    let mut message_forward =
                        bot.forward_messages(recepient.chat_id, from, message_ids);

                    if let Some(thread_id) = recepient.thread_id {
                        message_forward = message_forward.message_thread_id(thread_id);
                    }

                    message_forward.await.and(Ok(recepient.chat_id))
                });
            }

            while let Some(Ok(send_result)) = join_set.join_next().await {
                match send_result {
                    Ok(recepient_id) => {
                        tracing::info!(
                            "forwarded message to {recepient_id}",
                            recepient_id = recepient_id
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
        }

        Ok(())
    }
}

impl MessageInfo {
    pub const fn new(from: ChatId, id: MessageId, media_group_id: MediaGroupId) -> Self {
        Self {
            from,
            id,
            media_group_id,
        }
    }
}

fn seconds_since_unix_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_secs()
}
