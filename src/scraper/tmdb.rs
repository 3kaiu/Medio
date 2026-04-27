use crate::core::config::ApiConfig;
use crate::models::media::ScrapeResult;
use crate::models::media::ScrapeSource;
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone)]
pub struct TmdbScraper {
    client: Client,
    api_key: String,
    base_url: String,
}

impl TmdbScraper {
    pub fn new(config: &ApiConfig) -> Self {
        Self {
            client: Client::new(),
            api_key: config.tmdb_key.clone(),
            base_url: "https://api.themoviedb.org/3".into(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub async fn search_movie_candidates(
        &self,
        title: &str,
        year: Option<u16>,
        lang: Option<&str>,
        max: usize,
    ) -> Result<Vec<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(Vec::new());
        }

        let mut url = format!(
            "{}/search/movie?api_key={}&query={}",
            self.base_url,
            self.api_key,
            urlencoding::encode(title)
        );
        if let Some(y) = year {
            url.push_str(&format!("&year={y}"));
        }
        if let Some(lang) = lang {
            url.push_str(&format!("&language={lang}"));
        }

        let resp = self.client.get(&url).send().await?;
        let search: TmdbMovieSearchResponse = resp.json().await?;

        Ok(search
            .results
            .iter()
            .take(max)
            .map(|r| ScrapeResult {
                source: ScrapeSource::Tmdb,
                title: r.title.clone(),
                title_original: r.original_title.clone(),
                year: r
                    .release_date
                    .as_ref()
                    .and_then(|d| d.get(..4).and_then(|y| y.parse().ok())),
                overview: r.overview.clone(),
                rating: r.vote_average,
                season_number: None,
                episode_number: None,
                episode_name: None,
                poster_url: r
                    .poster_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}")),
                fanart_url: r
                    .backdrop_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}")),
                artist: None,
                album: None,
                track_number: None,
                author: None,
                cover_url: None,
                tmdb_id: Some(r.id as u64),
                musicbrainz_id: None,
                openlibrary_id: None,
            })
            .collect())
    }

    pub async fn search_tv_candidates(
        &self,
        title: &str,
        year: Option<u16>,
        lang: Option<&str>,
        max: usize,
    ) -> Result<Vec<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(Vec::new());
        }

        let mut url = format!(
            "{}/search/tv?api_key={}&query={}",
            self.base_url,
            self.api_key,
            urlencoding::encode(title)
        );
        if let Some(y) = year {
            url.push_str(&format!("&first_air_date_year={y}"));
        }
        if let Some(lang) = lang {
            url.push_str(&format!("&language={lang}"));
        }

        let resp = self.client.get(&url).send().await?;
        let search: TmdbTvSearchResponse = resp.json().await?;

        Ok(search
            .results
            .iter()
            .take(max)
            .map(|r| ScrapeResult {
                source: ScrapeSource::Tmdb,
                title: r.name.clone().unwrap_or_default(),
                title_original: r.original_name.clone(),
                year: r
                    .first_air_date
                    .as_ref()
                    .and_then(|d| d.get(..4).and_then(|y| y.parse().ok())),
                overview: r.overview.clone(),
                rating: r.vote_average,
                season_number: None,
                episode_number: None,
                episode_name: None,
                poster_url: r
                    .poster_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}")),
                fanart_url: r
                    .backdrop_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}")),
                artist: None,
                album: None,
                track_number: None,
                author: None,
                cover_url: None,
                tmdb_id: Some(r.id as u64),
                musicbrainz_id: None,
                openlibrary_id: None,
            })
            .collect())
    }

    pub async fn get_episode_with_lang(
        &self,
        tv_id: u64,
        season: u32,
        episode: u32,
        lang: Option<&str>,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let mut url = format!(
            "{}/tv/{}/season/{}/episode/{}?api_key={}",
            self.base_url, tv_id, season, episode, self.api_key
        );
        if let Some(lang) = lang {
            url.push_str(&format!("&language={lang}"));
        }
        let resp = self.client.get(&url).send().await?;
        let ep: TmdbEpisode = resp.json().await?;

        Ok(Some(ScrapeResult {
            source: ScrapeSource::Tmdb,
            title: String::new(),
            title_original: None,
            year: None,
            overview: ep.overview.clone(),
            rating: ep.vote_average,
            season_number: Some(season),
            episode_number: Some(episode),
            episode_name: Some(ep.name.clone()),
            poster_url: ep
                .still_path
                .as_ref()
                .map(|p| format!("https://image.tmdb.org/t/p/original{p}")),
            fanart_url: None,
            artist: None,
            album: None,
            track_number: None,
            author: None,
            cover_url: None,
            tmdb_id: Some(tv_id),
            musicbrainz_id: None,
            openlibrary_id: None,
        }))
    }
}

// --- TMDB API response types ---

#[derive(Debug, Deserialize)]
struct TmdbMovieSearchResponse {
    results: Vec<TmdbMovieItem>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvSearchResponse {
    results: Vec<TmdbTvItem>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovieItem {
    id: i64,
    title: String,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    vote_average: Option<f64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvItem {
    id: i64,
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    vote_average: Option<f64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbEpisode {
    name: String,
    overview: Option<String>,
    vote_average: Option<f64>,
    still_path: Option<String>,
}

// URL encoding helper (shared with other scrapers)
pub mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect()
    }
}
