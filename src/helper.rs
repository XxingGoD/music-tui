use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    config::{resolve_helper_path, AppConfig},
    models::{DownloadResponse, RemoteSong, SearchResponse},
};

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub songs: Vec<RemoteSong>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MusicDl {
    helper_path: PathBuf,
    source_cookies_json: String,
}

impl MusicDl {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            helper_path: resolve_helper_path(&config.helper_path),
            source_cookies_json: serde_json::to_string(&config.source_cookies)
                .unwrap_or_else(|_| "{}".to_string()),
        }
    }

    pub fn helper_path(&self) -> &Path {
        &self.helper_path
    }

    pub fn is_ready(&self) -> bool {
        self.helper_path.exists()
    }

    pub fn search(
        &self,
        keyword: &str,
        mode: &str,
        sources: &[String],
    ) -> Result<SearchResult, String> {
        let mut args = vec![
            OsString::from("search"),
            OsString::from("--keyword"),
            OsString::from(keyword),
            OsString::from("--mode"),
            OsString::from(mode),
            OsString::from("--limit"),
            OsString::from("120"),
        ];

        if !sources.is_empty() {
            args.push(OsString::from("--sources"));
            args.push(OsString::from(sources.join(",")));
        }

        let response: SearchResponse = self.run_json(args)?;
        Ok(SearchResult {
            songs: response.songs,
            warnings: response.warnings,
        })
    }

    pub fn download(
        &self,
        song: &RemoteSong,
        out_dir: &Path,
        cover: bool,
        lyrics: bool,
    ) -> Result<DownloadResponse, String> {
        let extra_json = serde_json::to_string(&song.extra).map_err(|err| err.to_string())?;
        let args = vec![
            OsString::from("download"),
            OsString::from("--id"),
            OsString::from(&song.id),
            OsString::from("--source"),
            OsString::from(&song.source),
            OsString::from("--name"),
            OsString::from(&song.name),
            OsString::from("--artist"),
            OsString::from(&song.artist),
            OsString::from("--album"),
            OsString::from(&song.album),
            OsString::from("--cover-url"),
            OsString::from(&song.cover),
            OsString::from("--url"),
            OsString::from(&song.url),
            OsString::from(format!("--cover={cover}")),
            OsString::from(format!("--lyrics={lyrics}")),
            OsString::from("--outdir"),
            out_dir.as_os_str().to_os_string(),
            OsString::from("--extra"),
            OsString::from(extra_json),
        ];

        self.run_json(args)
    }

    fn run_json<T>(&self, args: Vec<OsString>) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        let output = Command::new(&self.helper_path)
            .args(args)
            .env("MUSIC_TUI_SOURCE_COOKIES", &self.source_cookies_json)
            .output()
            .map_err(|err| format!("failed to run {}: {err}", self.helper_path.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(if detail.is_empty() {
                format!("helper exited with status {}", output.status)
            } else {
                detail
            });
        }

        serde_json::from_slice(&output.stdout).map_err(|err| {
            let body = String::from_utf8_lossy(&output.stdout);
            format!("invalid helper JSON: {err}; body={body}")
        })
    }
}
