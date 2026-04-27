use crate::models::media::{ScrapeResult, ScrapeSource};
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone)]
pub struct OpenLibraryScraper {
    client: Client,
    base_url: String,
}

impl OpenLibraryScraper {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: "https://openlibrary.org/search.json".into(),
        }
    }

    /// Search book by title + optional author
    pub async fn search(
        &self,
        title: &str,
        author: Option<&str>,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        let mut query = format!("title:\"{}\"", title);
        if let Some(a) = author {
            query.push_str(&format!(" AND author:\"{}\"", a));
        }

        let url = format!(
            "{}?q={}&limit=1",
            self.base_url,
            crate::scraper::tmdb::urlencoding::encode(&query)
        );

        let resp = self.client.get(&url).send().await?;
        let search: OlSearchResponse = resp.json().await?;

        let first = match search.docs.first() {
            Some(d) => d,
            None => return Ok(None),
        };

        let cover_url = first
            .cover_i
            .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-L.jpg"));

        Ok(Some(ScrapeResult {
            source: ScrapeSource::OpenLibrary,
            title: first.title.clone().unwrap_or_default(),
            title_original: None,
            year: first.first_publish_year,
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
            author: first.author_name.first().cloned(),
            cover_url,
            tmdb_id: None,
            musicbrainz_id: None,
            openlibrary_id: first.key.clone(),
        }))
    }
}

// --- OpenLibrary API response types ---

#[derive(Debug, Deserialize)]
struct OlSearchResponse {
    docs: Vec<OlDoc>,
}

#[derive(Debug, Deserialize)]
struct OlDoc {
    title: Option<String>,
    author_name: Vec<String>,
    first_publish_year: Option<u16>,
    cover_i: Option<u64>,
    key: Option<String>,
}
