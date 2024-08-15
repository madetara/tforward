use std::{
    fs::File,
    io::{BufReader, BufWriter},
    vec,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::RwLock};

pub struct Accessor {
    filepath: String,
    settings_cache: RwLock<SettingsCache>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub recepients: Vec<i64>,
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

    pub async fn get_settings(&self) -> Result<Settings> {
        self.load().await?;

        let settings_cache = self.settings_cache.read().await;

        Ok(settings_cache.settings.clone())
    }

    pub async fn add_recepient(&self, id: i64) -> Result<()> {
        let mut settings_cache = self.settings_cache.write().await;

        if !settings_cache.settings.recepients.contains(&id) {
            settings_cache.settings.recepients.push(id);
        }

        drop(settings_cache);

        self.flush().await?;

        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let f = File::open(&self.filepath)?;
        let writer = BufWriter::new(f);

        serde_json::to_writer(writer, &self.settings_cache.read().await.settings)?;

        Ok(())
    }

    async fn load(&self) -> Result<()> {
        let settings_cache = self.settings_cache.read().await;

        if settings_cache.loaded {
            return Ok(());
        }

        drop(settings_cache);

        let mut settings_cache = self.settings_cache.write().await;

        if settings_cache.loaded {
            return Ok(());
        }

        if fs::try_exists(&self.filepath).await? {
            let f = File::open(&self.filepath)?;
            let reader = BufReader::new(f);

            settings_cache.settings = serde_json::from_reader(reader)?;
        } else {
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
