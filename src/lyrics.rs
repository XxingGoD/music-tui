use std::{fs, path::Path, process::Command};

use serde_json::Value;

use crate::models::LyricLine;

const MIN_COMPLETE_LYRIC_LINES: usize = 3;

pub fn load_lyrics(audio_path: &Path) -> Vec<LyricLine> {
    let external = load_external_lyrics(audio_path);
    if external.len() >= MIN_COMPLETE_LYRIC_LINES {
        return external;
    }

    let embedded = load_embedded_lyrics(audio_path);
    if embedded.len() > external.len() {
        embedded
    } else {
        external
    }
}

fn load_external_lyrics(audio_path: &Path) -> Vec<LyricLine> {
    let candidates = [
        audio_path.with_extension("lrc"),
        audio_path.with_extension("txt"),
        audio_path.with_extension("lyric"),
    ];

    candidates
        .iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .map(|content| parse_lyrics(&content))
        .unwrap_or_default()
}

fn load_embedded_lyrics(audio_path: &Path) -> Vec<LyricLine> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(audio_path)
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let Ok(json) = serde_json::from_slice::<Value>(&output.stdout) else {
        return Vec::new();
    };

    extract_embedded_lyrics(&json)
        .map(|content| parse_lyrics(&content))
        .unwrap_or_default()
}

fn extract_embedded_lyrics(json: &Value) -> Option<String> {
    const KEYS: &[&str] = &[
        "lyrics",
        "LYRICS",
        "lyric",
        "LYRIC",
        "syncedlyrics",
        "SYNCEDLYRICS",
        "unsyncedlyrics",
        "UNSYNCEDLYRICS",
        "description",
        "DESCRIPTION",
    ];

    fn find_in_tags(tags: &Value) -> Option<String> {
        tags.as_object().and_then(|map| {
            KEYS.iter()
                .find_map(|key| map.get(*key).and_then(Value::as_str))
                .map(str::to_owned)
        })
    }

    find_in_tags(&json["format"]["tags"]).or_else(|| {
        json["streams"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|stream| find_in_tags(&stream["tags"]))
    })
}

fn parse_lyrics(content: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }

        let mut rest = line;
        let mut timestamps = Vec::new();
        while let Some(tagged) = rest.strip_prefix('[') {
            let Some(end) = tagged.find(']') else {
                break;
            };
            let tag = &tagged[..end];
            let Some(timestamp_ms) = parse_timestamp(tag) else {
                break;
            };
            timestamps.push(timestamp_ms);
            rest = &tagged[end + 1..];
        }

        let text = strip_inline_timestamps(rest).trim().to_string();
        if text.is_empty() || timestamps.is_empty() {
            continue;
        }

        for timestamp_ms in timestamps {
            lines.push(LyricLine {
                timestamp_ms,
                text: text.clone(),
            });
        }
    }

    if lines.is_empty() {
        return parse_plain_lyrics(content);
    }

    lines.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then_with(|| a.text.cmp(&b.text))
    });
    lines
}

fn strip_inline_timestamps(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;

    while let Some(start) = rest.find('[') {
        output.push_str(&rest[..start]);
        let tagged = &rest[start + 1..];
        let Some(end) = tagged.find(']') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let tag = &tagged[..end];
        if parse_timestamp(tag).is_some() {
            rest = &tagged[end + 1..];
        } else {
            output.push('[');
            rest = tagged;
        }
    }

    output.push_str(rest);
    output
}

fn parse_plain_lyrics(content: &str) -> Vec<LyricLine> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(index, text)| LyricLine {
            timestamp_ms: (index as u32).saturating_mul(5_000),
            text: text.to_string(),
        })
        .collect()
}

fn parse_timestamp(tag: &str) -> Option<u32> {
    let parts: Vec<&str> = tag.split(':').collect();
    let (hours, minutes, seconds_part) = match parts.as_slice() {
        [minutes, seconds] => (0, minutes.parse::<u32>().ok()?, *seconds),
        [hours, minutes, seconds] => (
            hours.parse::<u32>().ok()?,
            minutes.parse::<u32>().ok()?,
            *seconds,
        ),
        _ => return None,
    };

    let (seconds, millis) = match seconds_part.split_once('.') {
        Some((seconds, fraction)) => {
            let millis = normalize_fraction_ms(fraction)?;
            (seconds.parse::<u32>().ok()?, millis)
        }
        None => (seconds_part.parse::<u32>().ok()?, 0),
    };

    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

fn normalize_fraction_ms(fraction: &str) -> Option<u32> {
    let digits: String = fraction
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .take(3)
        .collect();
    match digits.len() {
        0 => Some(0),
        1 => Some(digits.parse::<u32>().ok()? * 100),
        2 => Some(digits.parse::<u32>().ok()? * 10),
        _ => digits.parse::<u32>().ok(),
    }
}
