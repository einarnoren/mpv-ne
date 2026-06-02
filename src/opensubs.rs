//! OpenSubtitles.com REST API client.
//!
//! Uses the free tier - requires a (free) API key from opensubtitles.com.
//! Without a key the search still works but rate limits are tighter.

use serde::Deserialize;

const BASE: &str = "https://api.opensubtitles.com/api/v1";
const APP_NAME: &str = "mpv-ne v0.1";
/// Default API key - users can override in settings.
const DEFAULT_KEY: &str = "Sv6cBrwMNbmfJrCv0lY71c7aO8M0xBpU";

#[derive(Debug, Clone)]
pub struct SubResult {
    #[allow(dead_code)]
    pub id:       String,
    pub language: String,
    pub release:  String,
    pub filename: String,
    pub downloads:u32,
    pub rating:   f32,
    pub file_id:  u64,
}

/// Search subtitles for a given query string.
pub async fn search(
    query: &str,
    language: &str,
    api_key: &str,
) -> anyhow::Result<Vec<SubResult>> {
    let key = if api_key.is_empty() { DEFAULT_KEY } else { api_key };
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{BASE}/subtitles"))
        .header("Api-Key", key)
        .header("User-Agent", APP_NAME)
        .header("Content-Type", "application/json")
        .query(&[
            ("query",    query),
            ("languages", language),
        ])
        .send()
        .await?
        .json::<SearchResp>()
        .await?;

    let results = resp.data.into_iter().filter_map(|item| {
        let attr = item.attributes;
        let file = attr.files.into_iter().next()?;
        Some(SubResult {
            id:        item.id,
            language:  attr.language,
            release:   attr.release.unwrap_or_default(),
            filename:  file.file_name.unwrap_or_default(),
            downloads: attr.download_count.unwrap_or(0),
            rating:    attr.ratings.unwrap_or(0.0),
            file_id:   file.file_id,
        })
    }).collect();

    Ok(results)
}

/// Get a one-time download URL for a subtitle file_id.
pub async fn download_url(file_id: u64, api_key: &str) -> anyhow::Result<String> {
    let key = if api_key.is_empty() { DEFAULT_KEY } else { api_key };
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "file_id": file_id });
    let resp: DownloadResp = client
        .post(format!("{BASE}/download"))
        .header("Api-Key", key)
        .header("User-Agent", APP_NAME)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    Ok(resp.link)
}

/// Download subtitle content to a temp file and return the path.
pub async fn download_to_temp(file_id: u64, filename: &str, api_key: &str) -> anyhow::Result<String> {
    let url   = download_url(file_id, api_key).await?;
    let bytes = reqwest::get(&url).await?.bytes().await?;
    let ext   = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("srt");
    let path  = std::env::temp_dir().join(format!("mpv-ne-sub.{ext}"));
    std::fs::write(&path, &bytes)?;
    Ok(path.to_string_lossy().into_owned())
}

// ── API response types ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchResp {
    data: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id:         String,
    attributes: SearchAttr,
}

#[derive(Deserialize)]
struct SearchAttr {
    language:       String,
    release:        Option<String>,
    download_count: Option<u32>,
    ratings:        Option<f32>,
    files:          Vec<SubFile>,
}

#[derive(Deserialize)]
struct SubFile {
    file_id:   u64,
    file_name: Option<String>,
}

#[derive(Deserialize)]
struct DownloadResp {
    link: String,
}
