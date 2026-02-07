//! Cache reader for Brain.fm
//!
//! Scans the Electron network cache for audio file URLs to determine
//! the currently playing track and its metadata.
//!
//! # Enrichment Strategy
//!
//! When an audio URL is found via `lsof`, we first try to look it up
//! in the API cache for rich, structured metadata (track name, genre,
//! NEL, activity). Only falls back to heuristic filename parsing when
//! no API cache match is available.

use anyhow::Result;
use log::debug;
use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use crate::api_cache_reader::ApiCacheData;
use crate::BrainFmState;

/// Regex for matching Brain.fm audio URLs in cache files
static AUDIO_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(https?://audio\d*\.brain\.fm/[^\s\x00"'<>]+\.mp3)"#).unwrap()
});

/// Regex for extracting .mp3 filename from a URL
static MP3_FILENAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/([^/?]+)\.mp3").unwrap()
});


use crate::util::url_decode;


/// Read state from Cache directory.
///
/// Accepts an optional `ApiCacheData` reference for enriching the detected
/// audio URL with structured metadata from cached API responses.
pub fn read_state(app_support_path: &Path, api_cache: Option<&mut ApiCacheData>) -> Result<BrainFmState> {
    let cache_path = app_support_path
        .join("Cache")
        .join("Cache_Data");

    if !cache_path.exists() {
        anyhow::bail!("Cache path not found: {:?}", cache_path);
    }

    let mut state = BrainFmState::new();

    // Use lsof as the authoritative play/pause signal.
    // When Brain.fm is playing, it holds Cache_Data file handles open.
    // When paused, it releases ALL Cache_Data handles (count drops to 0).
    match find_audio_url_via_lsof(&cache_path)? {
        Some(url) => {
            // lsof found open Cache_Data files with an audio URL = actively playing
            state = enrich_from_url(&url, state, api_cache);
            return Ok(state);
        }
        None => {
            // Check if Brain.fm has ANY Cache_Data files open (even without a parseable URL)
            if has_open_cache_files()? {
                // Process has cache files open but we couldn't extract a URL.
                // Fallback: scan cache files by access time.
                if let Some(url) = find_audio_url_by_atime(&cache_path)? {
                    state = enrich_from_url(&url, state, api_cache);
                }
            }
            // else: no Cache_Data files open at all = paused (is_playing stays false)
        }
    }

    Ok(state)
}

/// Enrich state from an audio URL.
///
/// Strategy:
/// 1. Try API cache lookup first (structured data, 100% accurate)
/// 2. Fall back to heuristic filename parsing (lossy but always available)
fn enrich_from_url(url: &str, mut state: BrainFmState, api_cache: Option<&mut ApiCacheData>) -> BrainFmState {
    // Strategy 1: API cache lookup (rich structured metadata)
    if let Some(cache) = api_cache {
        if let Some(metadata) = cache.lookup_by_url(url) {
            debug!("API cache hit for URL: track='{}'", metadata.name);
            state.track_name = Some(metadata.name.clone());
            state.genre = metadata.genre.clone();
            state.neural_effect = metadata.neural_effect.clone();
            state.mental_state_or_mode(&metadata);
            state.activity = metadata.activity.clone();
            state.image_url = metadata.image_url.clone();
            state.is_playing = true;
            return state;
        }
        debug!("API cache miss for URL, falling back to filename parsing");
    }

    // Strategy 2: Fallback to heuristic filename parsing
    parse_audio_url(url, state)
}

/// Check if Brain.fm has ANY Cache_Data files open (play/pause signal).
/// Returns true if at least one Cache_Data file handle is open.
/// When Brain.fm is paused, it releases ALL Cache_Data handles.
fn has_open_cache_files() -> Result<bool> {
    let output = crate::util::run_command_with_timeout(
        Command::new("lsof").args(["-c", "Brain.fm"]),
        crate::util::DEFAULT_COMMAND_TIMEOUT,
    )?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    Ok(stdout.lines().any(|line| line.contains("Cache_Data")))
}

/// Find audio URL by checking which cache file Brain.fm currently has open
/// This is the most reliable method - lsof shows exactly what's being read
fn find_audio_url_via_lsof(cache_path: &Path) -> Result<Option<String>> {
    let output = crate::util::run_command_with_timeout(
        Command::new("lsof").args(["-c", "Brain.fm"]),
        crate::util::DEFAULT_COMMAND_TIMEOUT,
    )?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Look for Cache_Data files that are open
    for line in stdout.lines() {
        if line.contains("Cache_Data") && line.contains("_0") {
            // Extract the filename from lsof output
            // Format: Brain.fm 1073 user 22u REG ... /path/to/file
            if let Some(path_start) = line.rfind('/') {
                let file_path = &line[path_start..];
                
                // Extract just the filename and read it
                if let Some(filename) = file_path.split('/').last() {
                    if filename.ends_with("_0") {
                        let file_to_read = cache_path.join(filename);
                        if file_to_read.exists() {
                            if let Ok(content) = fs::read(&file_to_read) {
                                let search_size = std::cmp::min(content.len(), 32768);
                                let content_str = String::from_utf8_lossy(&content[..search_size]);
                                
                                if let Some(caps) = AUDIO_URL_RE.captures(&content_str) {
                                    if let Some(url_match) = caps.get(1) {
                                        return Ok(Some(url_match.as_str().to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(None)
}

/// Fallback: Find audio URL by access time (less reliable due to kernel caching)
fn find_audio_url_by_atime(cache_path: &Path) -> Result<Option<String>> {
    let mut entries = fs::read_dir(cache_path)?
        .filter_map(|res| res.ok())
        .filter(|entry| {
            // Only look at _0 metadata files, not _s stream files
            // Stream files (_s) can have misleading access times
            entry.file_name().to_string_lossy().ends_with("_0")
        })
        .map(|entry| {
            let metadata = entry.metadata().ok()?;
            // Use access time (atime) instead of modification time
            // Access time updates when the file is read, which happens when a track plays
            let accessed = metadata.accessed().ok()?;
            Some((entry.path(), accessed))
        })
        .filter_map(|x| x)
        .collect::<Vec<_>>();
    
    // Sort by access time (most recently accessed first)
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Scan recent metadata files for audio URLs
    for (path, _) in entries.iter().take(100) {
        if let Ok(content) = fs::read(path) {
            // Search entire file content for audio URL (not just header)
            // Use chunks to avoid loading huge files entirely into string
            let search_size = std::cmp::min(content.len(), 32768); // 32KB should be enough
            let content_str = String::from_utf8_lossy(&content[..search_size]);
            
            // Look for brain.fm audio URLs - match various patterns
            if let Some(captures) = AUDIO_URL_RE.captures(&content_str) {
                if let Some(url_match) = captures.get(1) {
                    return Ok(Some(url_match.as_str().to_string()));
                }
            }
        }
    }
    
    Ok(None)
}

/// Parse metadata from audio URL
/// Examples: 
/// - https://audio2.brain.fm/Tied_In_Strings_Focus_Deep_Work_Electronic_30_120bpm_HighNEL_Nrmlzd2_VBR5.mp3
/// - https://audio2.brain.fm/Eternity%20Ringing%20Bowls%20Focus%201%20Conor_1.2_Nrmlzd2_VBR5.mp3
fn parse_audio_url(url: &str, mut state: BrainFmState) -> BrainFmState {
    let re = &*MP3_FILENAME_RE;
    
    if let Some(captures) = re.captures(url) {
        if let Some(filename) = captures.get(1) {
            // URL decode the filename (handle %20 etc)
            let filename_str = url_decode(filename.as_str());
            
            // Split by underscore first, but also handle spaces
            let parts: Vec<&str> = filename_str.split(|c| c == '_' || c == ' ')
                .filter(|s| !s.is_empty())
                .collect();
            
            // Known keywords that indicate end of track name
            // These mark the start of metadata in the filename
            let keywords: Vec<&str> = vec![
                // Modes
                "focus", "sleep", "relax", "meditate", "recharge",
                "deep", "light", "motivation",
                // Meditation types
                "unguided", "unguidedmeditation", "guided", "guidedmeditation",
                "meditation", "meditating",
                // Genres
                "piano", "electronic", "lofi", "ambient", "nature", "atmospheric",
                "grooves", "cinematic", "classical", "acoustic", "drone",
                "postrock", "chimes", "rain", "forest", "thunder",
                "beach", "night", "river", "wind", "underwater",
                // Technical metadata
                "conor", "nrmlzd", "nrmlzd2", "nrmlzd3", "vbr", "vbr5",
                "highnel", "mednel", "lownel",
            ];
            
            // Collect track name parts until we hit a keyword or metadata
            let mut track_name_parts: Vec<&str> = Vec::new();
            
            for part in parts.iter() {
                let lower = part.to_lowercase();
                
                // Stop at keywords
                if keywords.contains(&lower.as_str()) {
                    break;
                }
                
                // Stop at pure numbers (like "30" for duration)
                if part.chars().all(|c| c.is_numeric()) {
                    break;
                }
                
                // Stop at version numbers like "1.8", "1.9", "2.0"
                if lower.chars().next().map(|c| c.is_numeric()).unwrap_or(false) 
                    && lower.contains('.') {
                    break;
                }
                
                // Stop at duration patterns like "15mins", "60mins"
                if lower.ends_with("mins") || lower.ends_with("min") 
                    || lower.ends_with("bpm") {
                    break;
                }
                
                track_name_parts.push(part);
            }
            
            if !track_name_parts.is_empty() {
                // Join parts and handle CamelCase within each part
                let track_name: String = track_name_parts
                    .iter()
                    .map(|p| split_camel_case(p))
                    .collect::<Vec<_>>()
                    .join(" ");
                state.track_name = Some(track_name);
                state.is_playing = true;
            }
            
            // Try to map other parts loosely as the order might vary
            for part in &parts[1..] {
                let lower = part.to_lowercase();
                
                if lower == "focus" {
                    // Category, usually followed by specific mode
                } else if lower == "deepwork" {
                    state.mode = Some("Deep Work".to_string());
                } else if lower == "lightwork" {
                    state.mode = Some("Light Work".to_string());
                } else if lower == "motivation" {
                    state.mode = Some("Motivation".to_string());
                } else if lower == "sleep" {
                    state.mode = Some("Sleep".to_string());
                } else if lower == "relax" {
                    state.mode = Some("Relax".to_string());
                } else if lower == "meditation" || lower == "meditate" || lower == "meditating" 
                    || lower == "unguidedmeditation" || lower == "unguided" {
                    state.mode = Some("Meditate".to_string());
                } else if matches!(lower.as_str(), 
                    "piano" | "electronic" | "lofi" | "ambient" | "nature" | "atmospheric" 
                    | "grooves" | "cinematic" | "classical" | "acoustic" | "drone" 
                    | "post_rock" | "postrock" | "chimes" | "rain" | "forest" | "thunder"
                    | "beach" | "night" | "river" | "wind" | "underwater"
                ) {
                    // Capitalize first letter for display
                    let display_genre = capitalize_first(part);
                    state.genre = Some(display_genre);
                } else if lower.contains("highnel") {
                    state.neural_effect = Some("High Neural Effect".to_string());
                } else if lower.contains("mednel") {
                    state.neural_effect = Some("Medium Neural Effect".to_string());
                } else if lower.contains("lownel") {
                    state.neural_effect = Some("Low Neural Effect".to_string());
                } else if lower.starts_with("nrmlzd") {
                    // Newer format uses Nrmlzd2, Nrmlzd3 etc - indicates normalized audio
                    // If we see this and no NEL, set a generic neural effect
                    if state.neural_effect.is_none() {
                        state.neural_effect = Some("Neural Effect Level".to_string());
                    }
                }
            }
            
            // If we have mode but no neural effect, set a default based on mode
            if state.neural_effect.is_none() && state.mode.is_some() {
                state.neural_effect = Some("Neural Effect Active".to_string());
            }
        }
    }
    
    state
}

/// Helper to split CamelCase into words
/// "NothingRemains" -> "Nothing Remains"
fn split_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c.is_uppercase() && !result.is_empty() {
            // Check if previous char was uppercase (to handle acronyms correctly)
            // But simple CamelCase split: insert space before Uppercase if prev was not space
            result.push(' ');
        }
        result.push(c);
    }
    
    result
}

/// Capitalize first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url() {
        let url = "https://audio2.brain.fm/NothingRemains_Focus_DeepWork_Piano_30_90bpm_HighNEL_Nrmlzd2_VBR5.mp3?token=123";
        let state = parse_audio_url(url, BrainFmState::new());
        
        assert_eq!(state.track_name, Some("Nothing Remains".to_string()));
        assert_eq!(state.mode, Some("Deep Work".to_string()));
        assert_eq!(state.genre, Some("Piano".to_string()));
        assert_eq!(state.neural_effect, Some("High Neural Effect".to_string()));
    }
    
    #[test]
    fn test_camel_case() {
        assert_eq!(split_camel_case("NothingRemains"), "Nothing Remains");
        assert_eq!(split_camel_case("Simple"), "Simple");
        assert_eq!(split_camel_case("MyLongTrackName"), "My Long Track Name");
    }
}
