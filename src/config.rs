use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub music_dir: PathBuf,
    pub helper_path: PathBuf,
    pub default_sources: Vec<String>,
    pub embed_cover: bool,
    pub embed_lyrics: bool,
    pub source_cookies: HashMap<String, String>,
}

impl AppConfig {
    pub fn load() -> Self {
        let config_path = config_path();
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let default = Self::default();
            if let Ok(data) = toml::to_string_pretty(&default) {
                let _ = fs::write(&config_path, data);
            }
            return default;
        }

        fs::read_to_string(&config_path)
            .ok()
            .and_then(|data| toml::from_str(&data).ok())
            .unwrap_or_default()
    }

    pub fn ensure_dirs(&self) {
        let _ = fs::create_dir_all(&self.music_dir);
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            music_dir: home_dir().join("Music"),
            helper_path: PathBuf::from("helper").join(helper_binary_name()),
            default_sources: vec![
                "netease".to_string(),
                "qq".to_string(),
                "kugou".to_string(),
                "kuwo".to_string(),
                "migu".to_string(),
                "qianqian".to_string(),
                "soda".to_string(),
            ],
            embed_cover: true,
            embed_lyrics: true,
            source_cookies: HashMap::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    home_dir()
        .join(".config")
        .join("music-tui")
        .join("config.toml")
}

pub fn resolve_helper_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(path));
    }
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest_dir).join(path));
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join(path));
        candidates.push(exe_dir.join("..").join("..").join(path));
    }

    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
        #[cfg(target_os = "windows")]
        {
            if candidate.extension().is_none() {
                let windows_candidate = candidate.with_extension("exe");
                if windows_candidate.exists() {
                    return windows_candidate;
                }
            }
        }
    }

    path.to_path_buf()
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn helper_binary_name() -> &'static str {
    "music-dl-helper.exe"
}

#[cfg(not(target_os = "windows"))]
fn helper_binary_name() -> &'static str {
    "music-dl-helper"
}
