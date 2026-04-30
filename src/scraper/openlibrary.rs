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

        let mut result = ScrapeResult::empty(
            ScrapeSource::OpenLibrary,
            first.title.clone().unwrap_or_default(),
        )
        .with_confidence(0.83)
        .with_evidence([
            format!(
                "OpenLibrary work key={}",
                first.key.clone().unwrap_or_default()
            ),
            format!("query title '{}'", title),
        ]);
        result.year = first.first_publish_year;
        result.author = first.author_name.first().cloned();
        result.cover_url = cover_url;
        result.openlibrary_id = first.key.clone();
        Ok(Some(result))
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
