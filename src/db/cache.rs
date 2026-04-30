use crate::models::media::{ContentEvidence, HashInfo, ScanIndex, ScrapeResult};
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

    // --- Content probe cache ---

    pub fn get_content_probe(&self, key: &str) -> Option<ContentEvidence> {
        let full_key = format!("content_probe:{key}");
        let bytes = self.db.get(full_key).ok()??;
        Self::decode_entry(&bytes)
    }

    pub fn set_content_probe(&self, key: &str, result: &ContentEvidence) -> Result<(), CacheError> {
        let full_key = format!("content_probe:{key}");
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

    // --- Scan index cache ---

    pub fn get_scan_index_entry(&self, root: &str) -> Option<(u64, ScanIndex)> {
        let full_key = format!("scan:{root}");
        let bytes = self.db.get(full_key).ok()??;
        let entry = serde_json::from_slice::<CacheEntry<ScanIndex>>(&bytes).ok()?;
        Some((entry.updated_at, entry.value))
    }

    pub fn set_scan_index(&self, root: &str, index: &ScanIndex) -> Result<(), CacheError> {
        let full_key = format!("scan:{root}");
        let bytes = serde_json::to_vec(&CacheEntry {
            updated_at: current_unix_ts(),
            value: index.clone(),
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
    use crate::models::media::{MediaItem, MediaType, ScanIndex};
    use std::path::PathBuf;

    #[test]
    fn test_scrape_cache_backward_compatible() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sled")).unwrap();
        let key = "legacy";
        let full_key = format!("scrape:{key}");
        let payload = serde_json::json!({
            "source": "Guess",
            "title": "Legacy",
            "title_original": null,
            "year": 2020,
            "overview": null,
            "rating": null,
            "season_number": null,
            "episode_number": null,
            "episode_name": null,
            "poster_url": null,
            "fanart_url": null,
            "artist": null,
            "album": null,
            "track_number": null,
            "author": null,
            "cover_url": null,
            "tmdb_id": null,
            "musicbrainz_id": null,
            "openlibrary_id": null
        });

        cache
            .db
            .insert(full_key, serde_json::to_vec(&payload).unwrap())
            .unwrap();
        let loaded = cache.get_scrape(key).unwrap();
        assert_eq!(loaded.title, "Legacy");
        assert_eq!(loaded.confidence, 0.0);
        assert!(loaded.evidence.is_empty());
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

    #[test]
    fn test_scan_index_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::open(&dir.path().join("cache.sled")).unwrap();
        let index = ScanIndex {
            root: PathBuf::from("/media/demo"),
            items: vec![MediaItem {
                id: 1,
                path: PathBuf::from("/media/demo/show/01.mp4"),
                file_size: 123,
                media_type: MediaType::Movie,
                extension: "mp4".into(),
                parsed: None,
                quality: None,
                scraped: None,
                content_evidence: None,
                identity_resolution: None,
                hash: None,
                rename_plan: None,
            }],
        };

        cache.set_scan_index("/media/demo", &index).unwrap();
        let (updated_at, loaded_entry) = cache.get_scan_index_entry("/media/demo").unwrap();
        assert!(updated_at > 0);
        assert_eq!(loaded_entry.root, PathBuf::from("/media/demo"));
        assert_eq!(loaded_entry.items.len(), 1);
        assert_eq!(
            loaded_entry.items[0].path,
            PathBuf::from("/media/demo/show/01.mp4")
        );
    }
}
