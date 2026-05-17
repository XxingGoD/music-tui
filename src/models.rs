use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteSong {
    pub id: String,
    pub name: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub ext: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub extra: HashMap<String, String>,
    #[serde(default)]
    pub is_vip: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    pub songs: Vec<RemoteSong>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadResponse {
    #[serde(default)]
    pub status: String,
    pub path: PathBuf,
    pub filename: String,
    #[serde(default)]
    pub lyric_path: Option<PathBuf>,
    #[serde(default)]
    pub warning: String,
}

#[derive(Debug, Clone)]
pub struct LocalTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub timestamp_ms: u32,
    pub text: String,
}

pub fn format_duration(seconds: u32) -> String {
    if seconds == 0 {
        return "--:--".to_string();
    }
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
