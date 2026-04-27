use crate::models::media::{HashInfo, ScrapeResult};
use sled;
use std::path::Path;

#[allow(dead_code)]
pub struct Cache {
    db: sled::Db,
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum CacheError {
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[allow(dead_code)]
impl Cache {
    pub fn open(path: &Path) -> Result<Self, CacheError> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    // --- Scrape cache ---

    pub fn get_scrape(&self, key: &str) -> Option<ScrapeResult> {
        let full_key = format!("scrape:{key}");
        let bytes = self.db.get(full_key).ok()??;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn set_scrape(&self, key: &str, result: &ScrapeResult) -> Result<(), CacheError> {
        let full_key = format!("scrape:{key}");
        let bytes = serde_json::to_vec(result)?;
        self.db.insert(full_key, bytes)?;
        Ok(())
    }

    // --- Hash cache ---

    pub fn get_hash(&self, path: &str) -> Option<HashInfo> {
        let full_key = format!("hash:{path}");
        let bytes = self.db.get(full_key).ok()??;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn set_hash(&self, path: &str, info: &HashInfo) -> Result<(), CacheError> {
        let full_key = format!("hash:{path}");
        let bytes = serde_json::to_vec(info)?;
        self.db.insert(full_key, bytes)?;
        Ok(())
    }

    // --- TTL cleanup ---

    pub fn cleanup(&self, _ttl_days: u64) -> Result<u64, CacheError> {
        // TODO: implement TTL-based cleanup using embedded timestamps
        Ok(0)
    }

    /// Flush pending writes
    pub fn flush(&self) -> Result<(), CacheError> {
        self.db.flush()?;
        Ok(())
    }
}
