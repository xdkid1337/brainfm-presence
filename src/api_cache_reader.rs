//! API Cache Reader for Brain.fm
//!
//! Scans the Electron network cache for cached API responses from `api.brain.fm`.
//! These responses contain structured JSON with rich track metadata that is far
//! more reliable than parsing audio filenames.
//!
//! # Data Flow
//!
//! 1. Brain.fm Electron app makes HTTP requests to `api.brain.fm`
//! 2. Chromium caches these responses as `*_0` files in `Cache_Data/`
//! 3. Cache entries contain: HTTP headers + gzip-compressed JSON body
//! 4. We scan for `servings/recent` and `servings/favorites` endpoints
//! 5. We decompress and parse the JSON to build a filename → metadata lookup table
//! 6. The cache reader matches the currently playing audio URL against this table

use anyhow::Result;
use flate2::read::GzDecoder;
use log::{debug, trace};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

/// Rich metadata extracted from Brain.fm API responses
#[derive(Debug, Clone)]
pub struct TrackMetadata {
    /// Clean, human-readable track name (e.g., "Nothing Remains")
    pub name: String,

    /// Genre from track tags (e.g., "Electronic", "Piano", "Atmospheric")
    pub genre: Option<String>,

    /// Neural Effect Level as display string (e.g., "High Neural Effect")
    pub neural_effect: Option<String>,

    /// Numeric neural effect level (0.0 - 1.0)
    pub neural_effect_level: Option<f64>,

    /// Mental state / mode (e.g., "Focus", "Sleep", "Relax", "Meditate")
    pub mental_state: Option<String>,

    /// Activity within the mental state (e.g., "Deep Work", "Creativity", "Recharge")
    pub activity: Option<String>,

    /// Track image URL (usually Unsplash)
    pub image_url: Option<String>,

    /// Beats per minute
    pub bpm: Option<u32>,

    /// Mood tags (e.g., ["Calm", "Chill"])
    pub moods: Vec<String>,

    /// Instrument tags (e.g., ["Acoustic Piano", "Electronic Percussion"])
    pub instruments: Vec<String>,
}

/// Container for all API cache data, keyed by audio filename
#[derive(Debug)]
pub struct ApiCacheData {
    /// Maps audio filename (e.g., "Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3")
    /// to rich track metadata
    tracks: HashMap<String, TrackMetadata>,
}

impl ApiCacheData {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            tracks: HashMap::new(),
        }
    }

    /// Look up metadata by matching the audio URL's filename against cached data.
    ///
    /// The match is done against the filename portion of the URL (after the last `/`),
    /// stripping query parameters. This handles both URL-encoded and decoded filenames.
    pub fn lookup_by_url(&self, audio_url: &str) -> Option<&TrackMetadata> {
        let filename = extract_filename_from_url(audio_url)?;
        let decoded = url_decode(&filename);

        // Try exact match first (most common case)
        if let Some(meta) = self.tracks.get(&decoded) {
            return Some(meta);
        }

        // Try URL-encoded match
        if let Some(meta) = self.tracks.get(&filename) {
            return Some(meta);
        }

        // Substring match: check if any cached filename is contained in the URL
        // This handles edge cases where CDN prefixes differ
        for (cached_filename, meta) in &self.tracks {
            let decoded_cached = url_decode(cached_filename);
            if decoded.contains(&decoded_cached) || decoded_cached.contains(&decoded) {
                return Some(meta);
            }
        }

        None
    }

    /// Number of tracks in the cache
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Whether the cache is empty
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Merge another ApiCacheData into this one, overwriting existing entries.
    pub fn merge(&mut self, other: &ApiCacheData) {
        for (key, value) in &other.tracks {
            self.tracks.insert(key.clone(), value.clone());
        }
    }
}

impl Clone for ApiCacheData {
    fn clone(&self) -> Self {
        Self {
            tracks: self.tracks.clone(),
        }
    }
}

// --- JSON deserialization types for Brain.fm API responses ---

#[derive(Debug, Deserialize)]
struct ServingsResponse {
    result: Vec<Serving>,
}

#[derive(Debug, Deserialize)]
struct Serving {
    track: Track,
    #[serde(rename = "trackVariation")]
    track_variation: TrackVariation,
}

#[derive(Debug, Deserialize)]
struct Track {
    name: String,

    #[serde(default, rename = "beatsPerMinute")]
    beats_per_minute: Option<f64>,

    #[serde(default, rename = "imageUrl")]
    image_url: Option<String>,

    #[serde(default, rename = "mentalState")]
    mental_state: Option<MentalStateRef>,

    #[serde(default, rename = "mobileActivity")]
    mobile_activity: Option<ActivityRef>,

    #[serde(default)]
    tags: Vec<TrackTag>,
}

#[derive(Debug, Deserialize)]
struct TrackVariation {
    #[serde(default)]
    url: Option<String>,

    #[serde(default, rename = "neuralEffectLevel")]
    neural_effect_level: Option<f64>,

    #[serde(default, rename = "cdnUrl")]
    cdn_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MentalStateRef {
    #[serde(default, rename = "displayValue")]
    display_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActivityRef {
    #[serde(default, rename = "displayValue")]
    display_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrackTag {
    #[serde(rename = "type")]
    tag_type: String,
    value: String,
}

// --- Core functions ---

/// Read and parse all cached Brain.fm API responses from the Cache_Data directory.
///
/// Returns an `ApiCacheData` containing a lookup table of filename → metadata.
/// Safe to call even if no API data is cached — returns an empty table.
pub fn read_api_cache(app_support_path: &Path) -> Result<ApiCacheData> {
    let cache_path = app_support_path.join("Cache").join("Cache_Data");

    if !cache_path.exists() {
        debug!("Cache path not found: {:?}", cache_path);
        return Ok(ApiCacheData {
            tracks: HashMap::new(),
        });
    }

    let mut tracks = HashMap::new();

    // Scan all *_0 metadata files for API response patterns
    let entries = fs::read_dir(&cache_path)?;

    // Pre-compile regex for matching servings API URLs
    let servings_re =
        Regex::new(r"api\.brain\.fm/v3/users/[^/]+/servings/(recent|favorites)").unwrap();

    for entry in entries.flatten() {
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        // Only look at *_0 metadata files (not *_s stream files)
        if !filename_str.ends_with("_0") {
            continue;
        }

        // Quick check: read the first 512 bytes to check if it's an API response
        let file_path = entry.path();
        let data = match fs::read(&file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check the header area for our target URL pattern
        let header_size = std::cmp::min(data.len(), 512);
        let header_text = String::from_utf8_lossy(&data[..header_size]);

        if !servings_re.is_match(&header_text) {
            continue;
        }

        debug!("Found API cache entry: {:?}", file_path);

        // Try to extract and decompress the JSON body
        match extract_json_body(&data) {
            Some(json_body) => match parse_servings_response(&json_body) {
                Ok(parsed_tracks) => {
                    debug!(
                        "Parsed {} tracks from {:?}",
                        parsed_tracks.len(),
                        filename_str
                    );
                    tracks.extend(parsed_tracks);
                }
                Err(e) => {
                    trace!("Failed to parse JSON from {:?}: {}", filename_str, e);
                }
            },
            None => {
                trace!("Could not extract JSON body from {:?}", filename_str);
            }
        }
    }

    debug!("API cache: loaded {} tracks total", tracks.len());

    Ok(ApiCacheData { tracks })
}

/// Extract and decompress the JSON body from a Chromium cache entry.
///
/// Chromium cache files have: HTTP response metadata + optional gzip body.
/// We detect the gzip magic bytes (`1F 8B`) and decompress from there.
fn extract_json_body(data: &[u8]) -> Option<String> {
    // Strategy 1: Look for gzip magic bytes and decompress
    if let Some(pos) = find_gzip_start(data) {
        if let Ok(decompressed) = decompress_gzip(&data[pos..]) {
            return Some(decompressed);
        }
    }

    // Strategy 2: Look for raw JSON (non-compressed response)
    let text = String::from_utf8_lossy(data);
    if let Some(start) = text.find("{\"result\"") {
        // Find the end of the JSON by counting braces
        let json_candidate = &text[start..];
        if let Some(end) = find_json_end(json_candidate) {
            return Some(json_candidate[..end].to_string());
        }
    }

    None
}

/// Find the start position of gzip data (magic bytes 0x1F 0x8B)
fn find_gzip_start(data: &[u8]) -> Option<usize> {
    data.windows(2)
        .position(|w| w[0] == 0x1F && w[1] == 0x8B)
}

/// Decompress gzip data to a UTF-8 string
fn decompress_gzip(data: &[u8]) -> Result<String> {
    let mut decoder = GzDecoder::new(data);
    let mut output = String::new();
    decoder.read_to_string(&mut output)?;
    Ok(output)
}

/// Find the end of a JSON object by counting braces
fn find_json_end(json: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in json.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a Brain.fm servings JSON response into an `ApiCacheData` lookup table.
///
/// This is the public entry point for both the cache reader (which decompresses
/// cached responses) and the API client (which fetches live responses).
pub fn parse_servings_json(json_body: &str) -> Result<ApiCacheData> {
    let tracks = parse_servings_response(json_body)?;
    Ok(ApiCacheData { tracks })
}

/// Parse a Brain.fm servings API response and build a filename → metadata map
fn parse_servings_response(json_body: &str) -> Result<HashMap<String, TrackMetadata>> {
    let response: ServingsResponse = serde_json::from_str(json_body)?;
    let mut map = HashMap::new();

    for serving in response.result {
        let metadata = build_track_metadata(&serving.track, &serving.track_variation);

        // Key by the filename from trackVariation.url (just the filename, no CDN prefix)
        if let Some(ref url) = serving.track_variation.url {
            let decoded_url = url_decode(url);
            map.insert(decoded_url, metadata.clone());

            // Also key by the raw URL (before decoding) for encoded filenames
            if url != &url_decode(url) {
                map.insert(url.clone(), metadata.clone());
            }
        }

        // Also key by the CDN URL filename for broader matching
        if let Some(ref cdn_url) = serving.track_variation.cdn_url {
            if let Some(filename) = extract_filename_from_url(cdn_url) {
                let decoded = url_decode(&filename);
                if !map.contains_key(&decoded) {
                    map.insert(decoded, metadata);
                }
            }
        }
    }

    Ok(map)
}

/// Build a `TrackMetadata` from parsed API data
fn build_track_metadata(track: &Track, variation: &TrackVariation) -> TrackMetadata {
    // Extract genre from tags (first tag with type "genre", excluding "Nature")
    let genre = track
        .tags
        .iter()
        .find(|t| t.tag_type == "genre" && t.value != "Nature")
        .map(|t| t.value.clone());

    // Extract activity from tags
    let activity = track
        .tags
        .iter()
        .find(|t| t.tag_type == "activity")
        .map(|t| t.value.clone())
        // Fallback to mobileActivity.displayValue
        .or_else(|| {
            track
                .mobile_activity
                .as_ref()
                .and_then(|a| a.display_value.clone())
        });

    // Extract moods
    let moods: Vec<String> = track
        .tags
        .iter()
        .filter(|t| t.tag_type == "mood")
        .map(|t| t.value.clone())
        .collect();

    // Extract instruments
    let instruments: Vec<String> = track
        .tags
        .iter()
        .filter(|t| t.tag_type == "instrument")
        .map(|t| t.value.clone())
        .collect();

    // Convert NEL numeric to display string
    let neural_effect = variation
        .neural_effect_level
        .map(|nel| nel_display_value(nel));

    let mental_state = track
        .mental_state
        .as_ref()
        .and_then(|ms| ms.display_value.clone());

    TrackMetadata {
        name: track.name.clone(),
        genre,
        neural_effect,
        neural_effect_level: variation.neural_effect_level,
        mental_state,
        activity,
        image_url: track.image_url.clone(),
        bpm: track.beats_per_minute.map(|b| b as u32),
        moods,
        instruments,
    }
}

/// Convert Neural Effect Level from numeric (0.0-1.0) to display text.
///
/// This formula is extracted directly from Brain.fm's decompiled renderer JavaScript:
/// ```javascript
/// function getNelDisplayValue(e) {
///     return e <= .33 ? "Low" : e <= .66 ? "Medium" : "High";
/// }
/// ```
pub fn nel_display_value(level: f64) -> String {
    if level <= 0.33 {
        "Low Neural Effect".to_string()
    } else if level <= 0.66 {
        "Medium Neural Effect".to_string()
    } else {
        "High Neural Effect".to_string()
    }
}

/// Extract the filename portion from a URL (after the last `/`, before `?` query params)
fn extract_filename_from_url(url: &str) -> Option<String> {
    // Strip query parameters
    let path = url.split('?').next().unwrap_or(url);

    // Get the last path segment
    path.rsplit('/').next().map(|s| s.to_string())
}

/// Simple URL decode for common patterns
fn url_decode(s: &str) -> String {
    s.replace("%20", " ")
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%3D", "=")
        .replace("%26", "&")
        .replace("%2B", "+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nel_display_value() {
        assert_eq!(nel_display_value(0.0), "Low Neural Effect");
        assert_eq!(nel_display_value(0.2), "Low Neural Effect");
        assert_eq!(nel_display_value(0.33), "Low Neural Effect");
        assert_eq!(nel_display_value(0.34), "Medium Neural Effect");
        assert_eq!(nel_display_value(0.5), "Medium Neural Effect");
        assert_eq!(nel_display_value(0.66), "Medium Neural Effect");
        assert_eq!(nel_display_value(0.67), "High Neural Effect");
        assert_eq!(nel_display_value(0.79), "High Neural Effect");
        assert_eq!(nel_display_value(0.92), "High Neural Effect");
        assert_eq!(nel_display_value(1.0), "High Neural Effect");
    }

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            extract_filename_from_url(
                "https://audio2.brain.fm/Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3?token=abc"
            ),
            Some("Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3".to_string())
        );
        assert_eq!(
            extract_filename_from_url(
                "https://audio2.brain.fm/Stratosphere%20Relax%20Chill4%209hz%20Chris%2090bpm_60mins%201_60mins_VBR5.mp3"
            ),
            Some("Stratosphere%20Relax%20Chill4%209hz%20Chris%2090bpm_60mins%201_60mins_VBR5.mp3".to_string())
        );
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("Hello%20World"), "Hello World");
        assert_eq!(url_decode("no_encoding_here"), "no_encoding_here");
    }

    #[test]
    fn test_parse_servings_response() {
        let json = r#"{
            "result": [
                {
                    "track": {
                        "name": "Blooming",
                        "beatsPerMinute": 120,
                        "imageUrl": "https://images.unsplash.com/photo-123",
                        "mentalState": {
                            "displayValue": "Sleep"
                        },
                        "mobileActivity": {
                            "displayValue": "Deep Sleep"
                        },
                        "tags": [
                            { "type": "activity", "value": "Deep Sleep" },
                            { "type": "genre", "value": "Atmospheric" },
                            { "type": "instrument", "value": "Textural Soundscape" },
                            { "type": "mood", "value": "Calm" },
                            { "type": "mood", "value": "Chill" }
                        ]
                    },
                    "trackVariation": {
                        "url": "Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3",
                        "neuralEffectLevel": 0.92,
                        "cdnUrl": "https://audio2.brain.fm/Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3"
                    }
                }
            ]
        }"#;

        let tracks = parse_servings_response(json).unwrap();
        assert_eq!(tracks.len(), 1);

        let meta = tracks
            .get("Blooming_Sleep_DeepSleep_Atmospheric_60_120bpm_Nrmlzd2_VBR5.mp3")
            .expect("Should find by filename");

        assert_eq!(meta.name, "Blooming");
        assert_eq!(meta.genre, Some("Atmospheric".to_string()));
        assert_eq!(meta.neural_effect, Some("High Neural Effect".to_string()));
        assert_eq!(meta.neural_effect_level, Some(0.92));
        assert_eq!(meta.mental_state, Some("Sleep".to_string()));
        assert_eq!(meta.activity, Some("Deep Sleep".to_string()));
        assert_eq!(meta.bpm, Some(120));
        assert_eq!(meta.moods, vec!["Calm", "Chill"]);
        assert_eq!(meta.instruments, vec!["Textural Soundscape"]);
    }

    #[test]
    fn test_parse_url_encoded_filename() {
        let json = r#"{
            "result": [
                {
                    "track": {
                        "name": "Stratosphere",
                        "tags": []
                    },
                    "trackVariation": {
                        "url": "Stratosphere Relax Chill4 9hz Chris 90bpm_60mins 1_60mins_VBR5.mp3",
                        "neuralEffectLevel": 0.95,
                        "cdnUrl": "https://audio2.brain.fm/Stratosphere%20Relax%20Chill4%209hz%20Chris%2090bpm_60mins%201_60mins_VBR5.mp3"
                    }
                }
            ]
        }"#;

        let tracks = parse_servings_response(json).unwrap();

        // Should be findable by both decoded and CDN URL filename
        let meta = tracks
            .get("Stratosphere Relax Chill4 9hz Chris 90bpm_60mins 1_60mins_VBR5.mp3")
            .expect("Should find by decoded filename");

        assert_eq!(meta.name, "Stratosphere");
        assert_eq!(meta.neural_effect, Some("High Neural Effect".to_string()));
    }

    #[test]
    fn test_lookup_by_url() {
        let json = r#"{
            "result": [
                {
                    "track": {
                        "name": "Nine After Nine",
                        "tags": [
                            { "type": "genre", "value": "Electronic" },
                            { "type": "activity", "value": "Creativity" }
                        ]
                    },
                    "trackVariation": {
                        "url": "NineAfterNine_Focus_Electronic_Creativity_30_126BPM_HighNEL_Nrmlzd2_VBR5.mp3",
                        "neuralEffectLevel": 0.79,
                        "cdnUrl": "https://audio2.brain.fm/NineAfterNine_Focus_Electronic_Creativity_30_126BPM_HighNEL_Nrmlzd2_VBR5.mp3"
                    }
                }
            ]
        }"#;

        let tracks = parse_servings_response(json).unwrap();
        let cache = ApiCacheData { tracks };

        // Lookup by full CDN URL with query params (as found by lsof)
        let meta = cache
            .lookup_by_url("https://audio2.brain.fm/NineAfterNine_Focus_Electronic_Creativity_30_126BPM_HighNEL_Nrmlzd2_VBR5.mp3?expiration=123&token=abc")
            .expect("Should find by full URL");

        assert_eq!(meta.name, "Nine After Nine");
        assert_eq!(meta.genre, Some("Electronic".to_string()));
        assert_eq!(meta.activity, Some("Creativity".to_string()));
    }

    #[test]
    fn test_find_json_end() {
        assert_eq!(find_json_end(r#"{"a": "b"}"#), Some(10));
        assert_eq!(find_json_end(r#"{"a": {"b": "c"}}"#), Some(17));
        assert_eq!(find_json_end(r#"{"broken"#), None);
    }

    #[test]
    fn test_genre_excludes_nature() {
        let json = r#"{
            "result": [
                {
                    "track": {
                        "name": "Forest Walk",
                        "tags": [
                            { "type": "genre", "value": "Nature" },
                            { "type": "genre", "value": "Forest" }
                        ]
                    },
                    "trackVariation": {
                        "url": "ForestWalk_Sleep.mp3",
                        "neuralEffectLevel": 0.8
                    }
                }
            ]
        }"#;

        let tracks = parse_servings_response(json).unwrap();
        let meta = tracks.get("ForestWalk_Sleep.mp3").unwrap();

        // Should skip "Nature" and use "Forest" as the genre
        assert_eq!(meta.genre, Some("Forest".to_string()));
    }
}
