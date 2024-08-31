use std::{
    fs::File,
    io::{BufReader, BufWriter},
    vec,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use teloxide::types::{ChatId, ThreadId};
use tokio::{fs, sync::RwLock};
use tracing::instrument;

pub struct Accessor {
    filepath: String,
    settings_cache: RwLock<SettingsCache>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Settings {
    pub recepients: Vec<Recepient>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Recepient {
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
}

struct SettingsCache {
    pub loaded: bool,
    pub settings: Settings,
}

impl Accessor {
    pub fn new(filepath: &str) -> Self {
        let filepath = String::from(filepath);
        let settings_cache = RwLock::new(SettingsCache::new());
        Self {
            filepath,
            settings_cache,
        }
    }

    #[instrument(skip(self))]
    pub async fn get_settings(&self) -> Result<Settings> {
        self.load().await?;

        let settings_cache = self.settings_cache.read().await;

        Ok(settings_cache.settings.clone())
    }

    #[instrument(skip(self))]
    pub async fn add_recepient(&self, recepient: Recepient) -> Result<()> {
        let mut settings_cache = self.settings_cache.write().await;

        if !settings_cache.settings.recepients.contains(&recepient) {
            settings_cache.settings.recepients.push(recepient);
        }

        drop(settings_cache);

        self.flush().await?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn remove_recepient(&self, recepient: Recepient) -> Result<()> {
        let mut settings_cache = self.settings_cache.write().await;

        let new_recepients = settings_cache
            .settings
            .recepients
            .clone()
            .into_iter()
            .filter(|x| *x != recepient)
            .collect();

        settings_cache.settings.recepients = new_recepients;

        drop(settings_cache);

        self.flush().await?;

        Ok(())
    }

    #[instrument(name = "flush_settings", skip(self))]
    async fn flush(&self) -> Result<()> {
        let f = File::create(&self.filepath)?;
        let writer = BufWriter::new(f);

        serde_json::to_writer(writer, &self.settings_cache.read().await.settings)?;

        Ok(())
    }

    #[instrument(name = "load_settings", skip(self))]
    async fn load(&self) -> Result<()> {
        let settings_cache = self.settings_cache.read().await;

        if settings_cache.loaded {
            tracing::info!("load not needed");
            return Ok(());
        }

        drop(settings_cache);

        let mut settings_cache = self.settings_cache.write().await;

        if settings_cache.loaded {
            tracing::info!("load not needed");
            return Ok(());
        }

        if fs::try_exists(&self.filepath).await? {
            tracing::info!("loading from file: {}", self.filepath);
            let f = File::open(&self.filepath)?;
            let reader = BufReader::new(f);
            settings_cache.settings = serde_json::from_reader(reader)?;
        } else {
            tracing::info!("initializing new instance");
            settings_cache.settings = Settings::new();
        }

        settings_cache.loaded = true;

        drop(settings_cache);

        Ok(())
    }
}

impl Settings {
    pub const fn new() -> Self {
        Self { recepients: vec![] }
    }
}

impl SettingsCache {
    pub const fn new() -> Self {
        Self {
            loaded: false,
            settings: Settings::new(),
        }
    }
}
