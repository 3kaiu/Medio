use crate::core::config::AiConfig;
use crate::core::types::AiProvider;
use crate::models::media::{ScrapeResult, ScrapeSource};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// OpenAI-compatible chat completion client
/// Supports DeepSeek, Cloudflare Workers AI, and any OpenAI-compatible endpoint
#[derive(Clone)]
pub struct OpenAiCompat {
    client: Client,
    url: String,
    key: String,
    model: String,
}

impl OpenAiCompat {
    pub fn from_config(config: &AiConfig) -> Self {
        let (url, key, model) = match config.provider {
            AiProvider::DeepSeek => (
                config.deepseek.url.clone(),
                config.deepseek.key.clone(),
                config.deepseek.model.clone(),
            ),
            AiProvider::Cloudflare => (
                config.cloudflare.base_url(),
                config.cloudflare.api_token.clone(),
                config.cloudflare.model.clone(),
            ),
            AiProvider::Custom => (
                config.custom.url.clone(),
                config.custom.key.clone(),
                config.custom.model.clone(),
            ),
        };
        Self {
            client: Client::new(),
            url,
            key,
            model,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.url.is_empty()
            && !self.url.contains("{account_id}")
            && !self.key.is_empty()
            && !self.model.is_empty()
    }

    /// Identify media from filename using AI
    pub async fn identify(
        &self,
        filename: &str,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let system = "You are a media file identifier. Given a filename, extract the media metadata. \
            Respond ONLY with valid JSON: {\"title\":\"...\",\"year\":null,\"season\":null,\"episode\":null,\
            \"episode_name\":null,\"media_type\":\"movie|tv|music|novel\",\"artist\":null,\"album\":null,\
            \"author\":null}. If you cannot identify, return null for fields.";

        let user = format!("Identify this media file: {filename}");

        let resp = self.chat(system, &user).await?;

        // Parse the JSON from the response
        let json_str = extract_json(&resp);
        if json_str.is_empty() {
            return Ok(None);
        }

        let parsed: AiIdentifyResponse = match serde_json::from_str(&json_str) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        Ok(Some(ScrapeResult {
            source: ScrapeSource::AiAssist,
            title: parsed.title.clone().unwrap_or_default(),
            title_original: None,
            year: parsed.year,
            overview: None,
            rating: None,
            season_number: parsed.season,
            episode_number: parsed.episode,
            episode_name: parsed.episode_name,
            poster_url: None,
            fanart_url: None,
            artist: parsed.artist,
            album: parsed.album,
            track_number: None,
            author: parsed.author,
            cover_url: None,
            tmdb_id: None,
            musicbrainz_id: None,
            openlibrary_id: None,
        }))
    }

    /// Suggest a better title for ambiguous filenames
    pub async fn suggest_title(
        &self,
        filename: &str,
        current_title: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let system = "You are a media title expert. Given a filename and a current parsed title, \
            suggest a better title if the current one seems wrong or incomplete. \
            Respond with ONLY the suggested title, or 'SAME' if the current title is fine.";

        let user = format!("Filename: {filename}\nCurrent title: {current_title}");

        let resp = self.chat(system, &user).await?;
        let suggestion = resp.trim().to_string();

        if suggestion == "SAME" || suggestion == current_title {
            Ok(None)
        } else {
            Ok(Some(suggestion))
        }
    }

    /// Core chat completion request
    async fn chat(&self, system: &str, user: &str) -> Result<String, Box<dyn std::error::Error>> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                Message {
                    role: "system".into(),
                    content: system.into(),
                },
                Message {
                    role: "user".into(),
                    content: user.into(),
                },
            ],
            temperature: 0.1,
            max_tokens: 512,
        };

        let resp = self
            .client
            .post(&format!(
                "{}/chat/completions",
                self.url.trim_end_matches('/')
            ))
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("AI API error {status}: {body}").into());
        }

        let completion: ChatResponse = resp.json().await?;
        Ok(completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default())
    }
}

/// Extract JSON object from AI response (may be wrapped in markdown code blocks)
fn extract_json(s: &str) -> String {
    // Try to find ```json ... ``` block
    if let Some(start) = s.find("```json") {
        let json_start = start + 7;
        if let Some(end) = s[json_start..].find("```") {
            return s[json_start..json_start + end].trim().to_string();
        }
    }
    // Try to find raw { ... } block
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            if end > start {
                return s[start..=end].to_string();
            }
        }
    }
    String::new()
}

// --- API request/response types ---

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct AiIdentifyResponse {
    title: Option<String>,
    year: Option<u16>,
    season: Option<u32>,
    episode: Option<u32>,
    episode_name: Option<String>,
    #[allow(dead_code)]
    media_type: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    author: Option<String>,
}
