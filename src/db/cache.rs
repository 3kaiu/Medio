use crate::models::media::{HashInfo, ScrapeResult};
use serde::{Deserialize, Serialize};
use sled;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Cache {
    db: sled::Db,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry<T> {
    updated_at: u64,
    value: T,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self, CacheError> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    // --- Scrape cache ---

    pub fn get_scrape(&self, key: &str) -> Option<ScrapeResult> {
        let full_key = format!("scrape:{key}");
        let bytes = self.db.get(full_key).ok()??;
        Self::decode_entry(&bytes)
    }

    pub fn set_scrape(&self, key: &str, result: &ScrapeResult) -> Result<(), CacheError> {
        let full_key = format!("scrape:{key}");
        let bytes = serde_json::to_vec(&CacheEntry {
            updated_at: current_unix_ts(),
            value: result.clone(),
        })?;
        self.db.insert(full_key, bytes)?;
        Ok(())
    }

    // --- Hash cache ---

    pub fn get_hash(&self, path: &str) -> Option<HashInfo> {
        let full_key = format!("hash:{path}");
        let bytes = self.db.get(full_key).ok()??;
        Self::decode_entry(&bytes)
    }

    pub fn set_hash(&self, path: &str, info: &HashInfo) -> Result<(), CacheError> {
        let full_key = format!("hash:{path}");
        let bytes = serde_json::to_vec(&CacheEntry {
            updated_at: current_unix_ts(),
            value: info.clone(),
        })?;
        self.db.insert(full_key, bytes)?;
        Ok(())
    }

    // --- TTL cleanup ---

    pub fn cleanup(&self, ttl_days: u64) -> Result<u64, CacheError> {
        let ttl_secs = ttl_days.saturating_mul(24 * 60 * 60);
        if ttl_secs == 0 {
            return Ok(0);
        }

        let now = current_unix_ts();
        let mut removed = 0_u64;

        for entry in self.db.iter() {
            let (key, value) = entry?;

            let expired = Self::entry_updated_at(&value)
                .map(|updated_at| now.saturating_sub(updated_at) > ttl_secs)
                .unwrap_or(false);

            if expired {
                self.db.remove(key)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Flush pending writes
    pub fn flush(&self) -> Result<(), CacheError> {
        self.db.flush()?;
        Ok(())
    }

    fn decode_entry<T>(bytes: &[u8]) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice::<CacheEntry<T>>(bytes)
            .map(|entry| entry.value)
            .or_else(|_| serde_json::from_slice(bytes))
            .ok()
    }

    fn entry_updated_at(bytes: &[u8]) -> Option<u64> {
        serde_json::from_slice::<CacheEntry<serde_json::Value>>(bytes)
            .ok()
            .map(|entry| entry.updated_at)
    }
}

fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrape_cache_backward_compatible() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sled")).unwrap();
        let key = "legacy";
        let full_key = format!("scrape:{key}");
        let payload = ScrapeResult {
            source: crate::models::media::ScrapeSource::Guess,
            title: "Legacy".into(),
            title_original: None,
            year: Some(2020),
            overview: None,
            rating: None,
            season_number: None,
            episode_number: None,
            episode_name: None,
            poster_url: None,
            fanart_url: None,
            artist: None,
            album: None,
            track_number: None,
            author: None,
            cover_url: None,
            tmdb_id: None,
            musicbrainz_id: None,
            openlibrary_id: None,
        };

        cache.db.insert(full_key, serde_json::to_vec(&payload).unwrap()).unwrap();
        let loaded = cache.get_scrape(key).unwrap();
        assert_eq!(loaded.title, "Legacy");
    }

    #[test]
    fn test_cleanup_removes_expired_wrapped_entries() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sled")).unwrap();
        cache
            .db
            .insert(
                "scrape:old",
                serde_json::to_vec(&CacheEntry {
                    updated_at: 1,
                    value: serde_json::json!({"title":"old"}),
                })
                .unwrap(),
            )
            .unwrap();
        cache
            .db
            .insert(
                "scrape:new",
                serde_json::to_vec(&CacheEntry {
                    updated_at: current_unix_ts(),
                    value: serde_json::json!({"title":"new"}),
                })
                .unwrap(),
            )
            .unwrap();

        let removed = cache.cleanup(1).unwrap();
        assert_eq!(removed, 1);
        assert!(cache.db.get("scrape:old").unwrap().is_none());
        assert!(cache.db.get("scrape:new").unwrap().is_some());
    }
}
