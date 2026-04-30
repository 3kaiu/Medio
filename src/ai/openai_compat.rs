use crate::core::config::AiConfig;
use crate::core::types::AiProvider;
use crate::models::media::{ParsedInfo, ScrapeResult, ScrapeSource};
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
    #[allow(dead_code)]
    pub async fn identify(
        &self,
        filename: &str,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        self.identify_with_context(filename, &[], None).await
    }

    pub async fn identify_with_context(
        &self,
        filename: &str,
        parent_dirs: &[String],
        parsed: Option<&ParsedInfo>,
    ) -> Result<Option<ScrapeResult>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let system = "You are a media file identifier. Given a filename, extract the media metadata. \
            Respond ONLY with valid JSON: {\"title\":\"...\",\"year\":null,\"season\":null,\"episode\":null,\
            \"episode_name\":null,\"media_type\":\"movie|tv|music|novel\",\"artist\":null,\"album\":null,\
            \"author\":null}. If you cannot identify, return null for fields.";

        let mut user = format!("Identify this media file: {filename}");
        if !parent_dirs.is_empty() {
            user.push_str(&format!(
                "\nParent directories: {}",
                parent_dirs.join(" / ")
            ));
        }
        if let Some(parsed) = parsed {
            user.push_str(&format!(
                "\nCurrent parsed hints: title='{}', year={:?}, season={:?}, episode={:?}",
                parsed.raw_title, parsed.year, parsed.season, parsed.episode
            ));
        }

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

        let mut result = ScrapeResult::empty(
            ScrapeSource::AiAssist,
            parsed.title.clone().unwrap_or_default(),
        )
        .with_confidence(0.62)
        .with_evidence([
            "metadata generated from AI filename identification".to_string(),
            format!("input filename: {filename}"),
        ]);
        if !parent_dirs.is_empty() {
            result.push_evidence(format!("AI context parents: {}", parent_dirs.join(" / ")));
        }
        result.year = parsed.year;
        result.season_number = parsed.season;
        result.episode_number = parsed.episode;
        result.episode_name = parsed.episode_name;
        result.artist = parsed.artist;
        result.album = parsed.album;
        result.author = parsed.author;

        Ok(Some(result))
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
            .post(format!(
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
    if let Some(start) = s.find('{')
        && let Some(end) = s.rfind('}')
        && end > start
    {
        return s[start..=end].to_string();
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
