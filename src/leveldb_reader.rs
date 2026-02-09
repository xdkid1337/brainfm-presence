//! LevelDB reader for Brain.fm local storage
//!
//! Reads persistently stored data from the Electron app's LevelDB storage.

use crate::util::{KNOWN_GENRES, MODE_PATTERNS, MP3_FILENAME_RE};
use crate::BrainFmState;
use anyhow::Result;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

/// Regex for extracting display value from LevelDB content
static DISPLAY_VALUE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"displayValue["\s:\\]+([A-Za-z\s]+)"#).unwrap());

/// Regex for extracting track name from playback events
static TRACK_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""name"\s*:\s*"([^"]+)""#).unwrap());

/// Regex for extracting audio URL from playback events
static TRACK_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""url"\s*:\s*"([^"]+\.mp3[^"]*)""#).unwrap());

/// Read Brain.fm state from LevelDB files using strings extraction
///
/// Note: We use `strings` command because LevelDB files might be locked by the app.
/// This gives us read-only access to the stored data.
pub fn read_state(app_support_path: &Path) -> Result<BrainFmState> {
    let leveldb_path = app_support_path.join("Local Storage").join("leveldb");

    if !leveldb_path.exists() {
        anyhow::bail!("LevelDB path not found: {:?}", leveldb_path);
    }

    let mut state = BrainFmState::new();

    // Read strings from all LevelDB files using native Rust I/O
    let content = crate::util::read_leveldb_strings(&leveldb_path)?;

    // Parse the content for Brain.fm data
    state = parse_leveldb_content(&content, state);

    Ok(state)
}

/// Parse the extracted strings content for Brain.fm data
fn parse_leveldb_content(content: &str, mut state: BrainFmState) -> BrainFmState {
    // First, try to find the most recent playback event which has accurate track info
    // These events are logged with timestamps, so the last one in the file is the current
    state = parse_playback_events(content, state);

    // Look for activity/mode information

    // Only look for mode from persist:activities if we don't have it from playback events
    if state.mode.is_none() {
        // Try to find persist:activities data which contains the current mode
        if content.contains("persist:activities") {
            // Look for displayValue which contains the current mode
            if let Some(captures) = DISPLAY_VALUE_RE.captures(content) {
                if let Some(mode) = captures.get(1) {
                    let mode_str = mode.as_str().trim();
                    // Validate it's a known mode
                    for (pattern, name) in MODE_PATTERNS {
                        if mode_str.contains(pattern) {
                            state.mode = Some(name.to_string());
                            break;
                        }
                    }
                }
            }

            // Alternative: look for activity type tags
            if state.mode.is_none() {
                for (pattern, name) in MODE_PATTERNS {
                    if content.contains(&format!("y-{}", pattern.to_lowercase().replace(' ', "_")))
                        || content.contains(&format!("\"{}\"", pattern))
                    {
                        state.mode = Some(name.to_string());
                        break;
                    }
                }
            }
        }
    }

    // Check for ADHD mode
    if content.contains("\"isAdhdModeEnabled\":\"true\"")
        || content.contains("isAdhdModeEnabled\":true")
    {
        state.adhd_mode = true;
    }

    // Try to find session information
    // Look for patterns like "focus" or "sleep" in recent context
    if state.mode.is_none() {
        // Use simpler pattern matching
        let focus_indicators = [
            ("deep_work", "Deep Work"),
            ("light_work", "Light Work"),
            ("Deep Work", "Deep Work"),
            ("Light Work", "Light Work"),
        ];

        for (indicator, mode) in &focus_indicators {
            if content.contains(indicator) {
                state.mode = Some(mode.to_string());
                break;
            }
        }
    }

    state
}

/// Parse playback events to get the current track
/// These events contain the most accurate real-time track information
fn parse_playback_events(content: &str, mut state: BrainFmState) -> BrainFmState {
    // Find all core_playback_start_success events
    // The last one in the log is the most recent (current track)
    let mut last_track_name: Option<String> = None;
    let mut last_url: Option<String> = None;

    // Look for name and URL patterns near playback events
    for line in content.lines() {
        if line.contains("core_playback_start_success")
            || line.contains("core_playback_start_attempt")
        {
            // Extract name
            if let Some(caps) = TRACK_NAME_RE.captures(line) {
                if let Some(name) = caps.get(1) {
                    last_track_name = Some(name.as_str().to_string());
                }
            }
            // Extract URL
            if let Some(caps) = TRACK_URL_RE.captures(line) {
                if let Some(url) = caps.get(1) {
                    last_url = Some(url.as_str().to_string());
                }
            }
        }
    }

    // Apply found track info
    if let Some(track_name) = last_track_name {
        state.track_name = Some(track_name);
        state.is_playing = true;
    }

    // Parse URL for additional metadata (mode, genre, neural effect)
    if let Some(url) = last_url {
        state = parse_audio_url_for_metadata(&url, state);
    }

    state
}

/// Parse audio URL to extract metadata
fn parse_audio_url_for_metadata(url: &str, mut state: BrainFmState) -> BrainFmState {
    // Extract filename from URL
    if let Some(caps) = MP3_FILENAME_RE.captures(url) {
        if let Some(filename) = caps.get(1) {
            let parts: Vec<&str> = filename.as_str().split('_').collect();

            for part in &parts {
                let lower = part.to_lowercase();

                // Mode detection
                if state.mode.is_none() {
                    match lower.as_str() {
                        "deepwork" => state.mode = Some("Deep Work".to_string()),
                        "lightwork" => state.mode = Some("Light Work".to_string()),
                        "motivation" => state.mode = Some("Motivation".to_string()),
                        "sleep" => state.mode = Some("Sleep".to_string()),
                        "relax" => state.mode = Some("Relax".to_string()),
                        "meditation" | "meditate" => state.mode = Some("Meditate".to_string()),
                        _ => {}
                    }
                }

                // Genre detection
                if state.genre.is_none() {
                    if KNOWN_GENRES.contains(&lower.as_str()) {
                        let mut chars = part.chars();
                        let display = match chars.next() {
                            None => String::new(),
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                        };
                        state.genre = Some(display);
                    }
                }

                // Neural effect detection
                if state.neural_effect.is_none() {
                    if lower.contains("highnel") {
                        state.neural_effect = Some("High Neural Effect".to_string());
                    } else if lower.contains("mednel") {
                        state.neural_effect = Some("Medium Neural Effect".to_string());
                    } else if lower.contains("lownel") {
                        state.neural_effect = Some("Low Neural Effect".to_string());
                    }
                }
            }
        }
    }

    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deep_work() {
        let content = r#"persist:activities{"displayValue":"Deep Work"}"#;
        let state = parse_leveldb_content(content, BrainFmState::new());
        assert_eq!(state.mode, Some("Deep Work".to_string()));
    }

    #[test]
    fn test_parse_adhd_mode() {
        let content = r#"{"isAdhdModeEnabled":"true"}"#;
        let state = parse_leveldb_content(content, BrainFmState::new());
        assert!(state.adhd_mode);
    }
}
