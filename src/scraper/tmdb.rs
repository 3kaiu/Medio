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
            .map(|r| {
                let year = r
                    .release_date
                    .as_ref()
                    .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()));
                let mut result = ScrapeResult::empty(ScrapeSource::Tmdb, r.title.clone())
                    .with_confidence(0.9)
                    .with_evidence([
                        format!("TMDB movie candidate id={}", r.id),
                        format!("query title '{}'", title),
                    ]);
                result.title_original = r.original_title.clone();
                result.year = year;
                result.overview = r.overview.clone();
                result.rating = r.vote_average;
                result.poster_url = r
                    .poster_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}"));
                result.fanart_url = r
                    .backdrop_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}"));
                result.tmdb_id = Some(r.id as u64);
                if let Some(year) = year {
                    result.push_evidence(format!("TMDB release year {year}"));
                }
                result
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
            .map(|r| {
                let year = r
                    .first_air_date
                    .as_ref()
                    .and_then(|d| d.get(..4).and_then(|y| y.parse().ok()));
                let mut result =
                    ScrapeResult::empty(ScrapeSource::Tmdb, r.name.clone().unwrap_or_default())
                        .with_confidence(0.9)
                        .with_evidence([
                            format!("TMDB TV candidate id={}", r.id),
                            format!("query title '{}'", title),
                        ]);
                result.title_original = r.original_name.clone();
                result.year = year;
                result.overview = r.overview.clone();
                result.rating = r.vote_average;
                result.poster_url = r
                    .poster_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}"));
                result.fanart_url = r
                    .backdrop_path
                    .as_ref()
                    .map(|p| format!("https://image.tmdb.org/t/p/original{p}"));
                result.tmdb_id = Some(r.id as u64);
                if let Some(year) = year {
                    result.push_evidence(format!("TMDB first air year {year}"));
                }
                result
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

        let mut result = ScrapeResult::empty(ScrapeSource::Tmdb, "").with_confidence(0.94);
        result.overview = ep.overview.clone();
        result.rating = ep.vote_average;
        result.season_number = Some(season);
        result.episode_number = Some(episode);
        result.episode_name = Some(ep.name.clone());
        result.poster_url = ep
            .still_path
            .as_ref()
            .map(|p| format!("https://image.tmdb.org/t/p/original{p}"));
        result.tmdb_id = Some(tv_id);
        result.push_evidence(format!(
            "TMDB episode lookup tv_id={tv_id} season={season} episode={episode}"
        ));
        Ok(Some(result))
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
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    char::from(b).to_string()
                }
                _ => format!("%{:02X}", b),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn urlencoding_handles_utf8_titles() {
        assert_eq!(
            super::urlencoding::encode("财阀家的小儿子"),
            "%E8%B4%A2%E9%98%80%E5%AE%B6%E7%9A%84%E5%B0%8F%E5%84%BF%E5%AD%90"
        );
    }
}
