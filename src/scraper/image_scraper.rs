use reqwest::blocking::Client;
use std::path::{Path, PathBuf};

pub fn collect_urls(scraped: &crate::models::media::ScrapeResult) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(poster) = &scraped.poster_url {
        urls.push(poster.clone());
    }
    if let Some(fanart) = &scraped.fanart_url {
        urls.push(fanart.clone());
    }
    if let Some(cover) = &scraped.cover_url {
        urls.push(cover.clone());
    }
    urls
}

pub fn build_image_path(target_dir: &Path, index: usize, url: &str) -> PathBuf {
    let name = match index {
        0 => "poster",
        1 => "fanart",
        _ => "image",
    };
    let ext = if url.contains(".png") { "png" } else { "jpg" };
    target_dir.join(format!("{name}.{ext}"))
}

pub fn download(client: &Client, url: &str, dest: &Path) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("download {url}: {e}"))?;
    let bytes = response
        .bytes()
        .map_err(|e| format!("read bytes from {url}: {e}"))?;
    std::fs::write(dest, &bytes).map_err(|e| format!("write image {}: {e}", dest.display()))
}
