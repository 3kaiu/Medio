use crate::core::config::AiConfig;
use crate::models::media::ScrapeResult;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Embedding client for re-ranking scrape candidates
#[derive(Clone)]
pub struct EmbeddingClient {
    client: Client,
    url: String,
    key: String,
    model: String,
}

impl EmbeddingClient {
    pub fn from_config(config: &AiConfig) -> Self {
        let (url, key, model) = match config.embedding_provider {
            crate::core::types::AiProvider::DeepSeek => (
                config.deepseek.url.clone(),
                config.deepseek.key.clone(),
                if config.embedding_model.is_empty() {
                    config.deepseek.model.clone()
                } else {
                    config.embedding_model.clone()
                },
            ),
            crate::core::types::AiProvider::Cloudflare => (
                config.cloudflare.url.clone(),
                config.cloudflare.api_token.clone(),
                if config.embedding_model.is_empty() {
                    config.cloudflare.model.clone()
                } else {
                    config.embedding_model.clone()
                },
            ),
            crate::core::types::AiProvider::Custom => (
                config.custom.url.clone(),
                config.custom.key.clone(),
                if config.embedding_model.is_empty() {
                    config.custom.model.clone()
                } else {
                    config.embedding_model.clone()
                },
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
        !self.url.is_empty() && !self.key.is_empty() && !self.model.is_empty()
    }

    /// Get embedding vector for a single text
    #[allow(dead_code)]
    pub async fn embed(&self, text: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        let results = self.embed_batch(&[text]).await?;
        Ok(results.into_iter().next().unwrap_or_default())
    }

    /// Get embedding vectors for multiple texts in a single API call
    pub async fn embed_batch(
        &self,
        texts: &[&str],
    ) -> Result<Vec<Vec<f64>>, Box<dyn std::error::Error>> {
        if !self.is_configured() {
            return Err("Embedding client not configured".into());
        }
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        let resp = self
            .client
            .post(&format!("{}/embeddings", self.url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Embedding API error {status}: {body}").into());
        }

        let result: EmbeddingResponse = resp.json().await?;
        // Sort by index to ensure order matches input
        let mut data = result.data;
        data.sort_by_key(|d| d.index.unwrap_or(0));
        Ok(data.iter().map(|d| d.embedding.clone()).collect())
    }

    /// Re-rank scrape candidates by embedding similarity to the query
    /// Uses a single batch API call for all embeddings (query + candidates)
    pub async fn rerank(
        &self,
        query: &str,
        candidates: &[ScrapeResult],
    ) -> Result<Vec<(usize, f64)>, Box<dyn std::error::Error>> {
        if !self.is_configured() || candidates.is_empty() {
            return Ok(candidates
                .iter()
                .enumerate()
                .map(|(i, _)| (i, 0.0))
                .collect());
        }

        // Build all texts: [query, candidate_0, candidate_1, ...]
        let mut texts: Vec<String> = vec![query.to_string()];
        for candidate in candidates {
            texts.push(format!(
                "{} {} {}",
                candidate.title,
                candidate.year.map(|y| y.to_string()).unwrap_or_default(),
                candidate.overview.as_deref().unwrap_or_default()
            ));
        }
        let text_refs: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();

        // Single batch API call
        let embeddings = self.embed_batch(&text_refs).await?;
        if embeddings.is_empty() {
            return Ok(candidates
                .iter()
                .enumerate()
                .map(|(i, _)| (i, 0.0))
                .collect());
        }

        let query_emb = &embeddings[0];
        let query_norm = query_emb.iter().map(|x| x * x).sum::<f64>().sqrt();

        let mut scored = Vec::with_capacity(candidates.len());
        for (i, cand_emb) in embeddings.iter().skip(1).enumerate() {
            let sim = cosine_similarity_precomputed(query_emb, query_norm, cand_emb);
            scored.push((i, sim));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored)
    }
}

#[allow(dead_code)]
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Cosine similarity with precomputed norm for vector `a`
pub fn cosine_similarity_precomputed(a: &[f64], norm_a: f64, b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() || norm_a == 0.0 {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// --- API types ---

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f64>,
    index: Option<usize>,
}
