use anyhow::Result;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use teloxide::{
    prelude::Requester,
    types::{ChatId, MessageId},
    Bot,
};
use tokio::{
    sync::{
        mpsc::{error::TryRecvError, UnboundedReceiver, UnboundedSender},
        Mutex,
    },
    task::JoinSet,
    time::sleep,
};

use super::settings::Accessor;

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct MediaGroupId(pub String);

#[derive(Clone)]
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
        while !self.reciever.0.is_closed() {
            match self.reciever.0.try_recv() {
                Ok(message_info) => {
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
                Err(TryRecvError::Empty) => {
                    if let Err(error) = self.try_send_messages().await {
                        tracing::warn!("error occured while sending messages, details: {}", error);
                    }
                    sleep(PAUSE_DURATION).await;
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    async fn try_send_messages(&self) -> Result<()> {
        let current_time = seconds_since_unix_epoch();
        let mut to_remove = vec![];
        let send_plan = Arc::clone(&self.send_plan);
        let send_plan_lock = send_plan.lock().await;

        for (media_group_id, media_group_info) in send_plan_lock.iter() {
            let media_group_info = Arc::clone(media_group_info);
            let media_group_info_lock = media_group_info.lock().await;

            if current_time - media_group_info_lock.last_message_timestamp
                < MESSAGE_SEND_DELAY_SECONDS
            {
                continue;
            }

            drop(media_group_info_lock);

            let recepients = self.settings.get_settings().await?.recepients;

            let mut join_set = JoinSet::new();

            for id in recepients.into_iter().map(ChatId) {
                tracing::info!("forwarding message to {recepient_id}", recepient_id = id);
                let bot = self.bot.clone();
                let media_group_info = Arc::clone(&media_group_info);

                join_set.spawn(async move {
                    let media_group_info = media_group_info.lock().await;

                    bot.forward_messages(
                        id,
                        media_group_info.from,
                        media_group_info.message_ids.clone(),
                    )
                    .await
                    .and(Ok(id))
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

            to_remove.push(media_group_id.clone());
        }

        drop(send_plan_lock);

        let send_plan = Arc::clone(&self.send_plan);
        for media_group_id in to_remove {
            let mut send_plan_lock = send_plan.lock().await;
            send_plan_lock.remove(&media_group_id);
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
