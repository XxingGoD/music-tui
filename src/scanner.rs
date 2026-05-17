use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;

use crate::models::LocalTrack;

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "ogg", "wav", "wma", "aac", "dsf", "dff",
];

pub fn scan_music_dir(root: &Path) -> Vec<LocalTrack> {
    let mut files = Vec::new();
    collect_audio_files(root, &mut files);

    let mut tracks: Vec<LocalTrack> = files.into_iter().map(|path| probe_track(path)).collect();
    tracks.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
    });
    tracks
}

fn collect_audio_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_files(&path, files);
        } else if is_audio_file(&path) {
            files.push(path);
        }
    }
}

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn probe_track(path: PathBuf) -> LocalTrack {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(&path)
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<Value>(&output.stdout) {
                return track_from_probe(path, &json);
            }
        }
    }

    fallback_track(path)
}

fn track_from_probe(path: PathBuf, json: &Value) -> LocalTrack {
    let tags = &json["format"]["tags"];
    let title = tag(tags, "title").unwrap_or_else(|| file_stem(&path));
    let artist = tag(tags, "artist").unwrap_or_else(|| "Unknown Artist".to_string());
    let album = tag(tags, "album").unwrap_or_default();
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|duration| duration.parse::<f64>().ok())
        .unwrap_or_default() as u32;

    LocalTrack {
        path,
        title,
        artist,
        album,
        duration,
    }
}

fn fallback_track(path: PathBuf) -> LocalTrack {
    LocalTrack {
        title: file_stem(&path),
        artist: "Unknown Artist".to_string(),
        album: String::new(),
        duration: 0,
        path,
    }
}

fn tag(tags: &Value, key: &str) -> Option<String> {
    tags.get(key)
        .or_else(|| tags.get(key.to_ascii_uppercase()))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Unknown")
        .to_string()
}
