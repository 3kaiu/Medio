use crate::core::config::ApiConfig;
use crate::models::media::{ScrapeResult, ScrapeSource};
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone)]
pub struct MusicBrainzScraper {
    client: Client,
    user_agent: String,
    base_url: String,
}

impl MusicBrainzScraper {
    pub fn new(config: &ApiConfig) -> Self {
        Self {
            client: Client::new(),
            user_agent: config.musicbrainz_user_agent.clone(),
            base_url: "https://musicbrainz.org/ws/2".into(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.user_agent.is_empty()
    }

    /// Search recording by artist + title
    pub async fn search_recording(
        &self,
        artist: &str,
        title: &str,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let query = format!("artist:\"{}\" AND recording:\"{}\"", artist, title);
        let url = format!(
            "{}/recording?query={}&fmt=json&limit=1",
            self.base_url,
            crate::scraper::tmdb::urlencoding::encode(&query)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        let search: MbRecordingResponse = resp.json().await?;

        let first = match search.recordings.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        let artist_name = first
            .artist_credit
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let release = first.releases.first();

        Ok(Some(ScrapeResult {
            source: ScrapeSource::MusicBrainz,
            title: first.title.clone(),
            title_original: None,
            year: release
                .and_then(|r| r.date.as_ref())
                .and_then(|d| d.get(..4).and_then(|y| y.parse().ok())),
            overview: None,
            rating: None,
            season_number: None,
            episode_number: None,
            episode_name: None,
            poster_url: None,
            fanart_url: None,
            artist: Some(artist_name),
            album: release.map(|r| r.title.clone()),
            track_number: None,
            author: None,
            cover_url: None,
            tmdb_id: None,
            musicbrainz_id: Some(first.id.clone()),
            openlibrary_id: None,
        }))
    }

    /// Search release (album) by artist + title
    #[allow(dead_code)]
    pub async fn search_release(
        &self,
        artist: &str,
        album: &str,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let query = format!("artist:\"{}\" AND release:\"{}\"", artist, album);
        let url = format!(
            "{}/release?query={}&fmt=json&limit=1",
            self.base_url,
            crate::scraper::tmdb::urlencoding::encode(&query)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", &self.user_agent)
            .send()
            .await?;

        let search: MbReleaseResponse = resp.json().await?;

        let first = match search.releases.first() {
            Some(r) => r,
            None => return Ok(None),
        };

        let artist_name = first
            .artist_credit
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default();

        Ok(Some(ScrapeResult {
            source: ScrapeSource::MusicBrainz,
            title: first.title.clone(),
            title_original: None,
            year: first
                .date
                .as_ref()
                .and_then(|d| d.get(..4).and_then(|y| y.parse().ok())),
            overview: None,
            rating: None,
            season_number: None,
            episode_number: None,
            episode_name: None,
            poster_url: None,
            fanart_url: None,
            artist: Some(artist_name),
            album: Some(first.title.clone()),
            track_number: None,
            author: None,
            cover_url: None,
            tmdb_id: None,
            musicbrainz_id: Some(first.id.clone()),
            openlibrary_id: None,
        }))
    }
}

// --- MusicBrainz API response types ---

#[derive(Debug, Deserialize)]
struct MbRecordingResponse {
    recordings: Vec<MbRecording>,
}

#[derive(Debug, Deserialize)]
struct MbRecording {
    id: String,
    title: String,
    artist_credit: Vec<MbArtistCredit>,
    releases: Vec<MbRecordingRelease>,
}

#[derive(Debug, Deserialize)]
struct MbArtistCredit {
    name: String,
}

#[derive(Debug, Deserialize)]
struct MbRecordingRelease {
    title: String,
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MbReleaseResponse {
    releases: Vec<MbRelease>,
}

#[derive(Debug, Deserialize)]
struct MbRelease {
    id: String,
    title: String,
    date: Option<String>,
    artist_credit: Vec<MbArtistCredit>,
}
