#![deny(unsafe_code)]

//! Brain.fm information reader
//!
//! This module provides functionality to read the current state of Brain.fm app
//! including the active mode (Deep Work, Light Work, etc.), current track, and session time.
//!
//! # Data Sources (in priority order)
//! 1. **Direct API** — Live HTTP call to `api.brain.fm` using JWT from LevelDB (best quality)
//! 2. **API Cache** — Fallback: structured JSON from cached API responses
//! 3. **Cache Reader** — Audio URL parsing via `lsof` (real-time play/pause detection)
//! 4. **LevelDB** — Persisted Redux state (baseline data, may be stale)

use anyhow::Result;
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod api_cache_reader;
pub mod api_client;
pub mod cache_reader;
pub mod leveldb_reader;
pub mod media_remote_reader;
pub mod platform;
pub mod util;

/// Represents the current state of Brain.fm playback
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainFmState {
    /// Current mental state mode (e.g., "Focus", "Sleep", "Relax", "Meditate")
    pub mode: Option<String>,

    /// Whether currently playing
    pub is_playing: bool,

    /// Current track name (e.g., "Nothing Remains", "Blooming")
    pub track_name: Option<String>,

    /// Neural effect level display text (e.g., "High Neural Effect")
    pub neural_effect: Option<String>,

    /// Genre (e.g., "Piano", "Electronic", "Atmospheric")
    pub genre: Option<String>,

    /// Activity within the mode (e.g., "Deep Work", "Creativity", "Recharge")
    pub activity: Option<String>,

    /// Track image URL (usually from Unsplash, used for Discord large image)
    pub image_url: Option<String>,

    /// Session state (e.g., "IN FOCUS")
    pub session_state: Option<String>,

    /// Time in current session (formatted as "H:MM:SS")
    pub session_time: Option<String>,

    /// Whether infinite play is enabled
    pub infinite_play: bool,

    /// Whether ADHD mode is enabled
    pub adhd_mode: bool,
}

impl BrainFmState {
    /// Create a new empty state
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if Brain.fm is actively playing
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.is_playing && self.mode.is_some()
    }

    /// Set mode from API cache metadata.
    ///
    /// The API distinguishes between "mental state" (Focus, Sleep, Relax, Meditate)
    /// and "activity" (Deep Work, Creativity, Recharge, etc.).
    /// For Discord presence, we use the activity as the mode when it's a known
    /// sub-mode, and fall back to the mental state.
    pub fn mental_state_or_mode(&mut self, metadata: &crate::api_cache_reader::TrackMetadata) {
        // Use the activity as our display mode if it's specific enough
        if let Some(ref activity) = metadata.activity {
            self.mode = Some(activity.clone());
        } else if let Some(ref ms) = metadata.mental_state {
            self.mode = Some(ms.clone());
        }
    }

    /// Get a display string for Discord Rich Presence
    pub fn to_presence_string(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref mode) = self.mode {
            parts.push(mode.clone());
        }

        if let Some(ref state) = self.session_state {
            parts.push(format!("({})", state));
        }

        if let Some(ref time) = self.session_time {
            parts.push(format!("[{}]", time));
        }

        if parts.is_empty() {
            "Brain.fm".to_string()
        } else {
            parts.join(" ")
        }
    }

    /// Get details string for Discord Rich Presence.
    ///
    /// Format: "Track Name • Genre • Neural Effect"
    /// Example: "Nothing Remains • Piano • High Neural Effect"
    pub fn to_details_string(&self) -> Option<String> {
        let mut parts = Vec::new();

        if let Some(ref track) = self.track_name {
            parts.push(track.clone());
        }

        if let Some(ref genre) = self.genre {
            parts.push(genre.clone());
        }

        if let Some(ref effect) = self.neural_effect {
            parts.push(effect.clone());
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" • "))
        }
    }
}

/// Number of read_state cycles between periodic API refreshes.
/// With a 5-second update interval, this means ~30 seconds between refreshes.
const API_REFRESH_INTERVAL: u32 = 6;

/// Main reader that combines multiple data sources
pub struct BrainFmReader {
    /// Path to Brain.fm app support directory
    app_support_path: PathBuf,

    /// In-memory cache of API responses to persist metadata even if token expires
    memory_cache: api_cache_reader::ApiCacheData,

    /// Counts cycles since the last successful API call.
    /// When this reaches `API_REFRESH_INTERVAL`, a periodic refresh is triggered.
    api_refresh_counter: u32,

    /// The audio URL (or track name) that was last enriched via the Direct API.
    /// Used to detect track changes and trigger immediate API calls.
    last_api_track: Option<String>,
}

impl BrainFmReader {
    /// Create a new reader
    pub fn new() -> Result<Self> {
        let app_support_path = platform::get_brainfm_data_dir()?;
        let memory_cache = api_cache_reader::ApiCacheData::new();
        Ok(Self {
            app_support_path,
            memory_cache,
            api_refresh_counter: API_REFRESH_INTERVAL, // trigger API on first cycle
            last_api_track: None,
        })
    }

    /// Check if Brain.fm is running
    pub fn is_running(&self) -> bool {
        platform::is_brainfm_running()
    }

    /// Read current state using all available methods.
    ///
    /// Priority order:
    /// 1. LevelDB — baseline data (mode, ADHD mode), may be stale
    /// 2. Cache Reader — real-time audio URL detection via `lsof`
    /// 3. Direct API — called on track change or periodic refresh for fresh metadata
    /// 4. Memory Cache + Disk cache — fallback when API is unavailable
    /// 5. MediaRemote — macOS Now Playing fallback when `lsof` detection fails
    pub fn read_state(&mut self) -> Result<BrainFmState> {
        let mut state = BrainFmState::new();

        // Check if app is running
        if !self.is_running() {
            return Ok(state);
        }

        // 1. LevelDB (baseline data, may be stale)
        if let Ok(leveldb_state) = self.read_from_leveldb() {
            state = Self::merge_state(state, leveldb_state);
        }

        // 2. Fast path: if we already have complete metadata in memory cache
        //    for the current track, just use MediaRemote for play/pause detection
        //    and skip expensive disk cache parsing + lsof scanning.
        if !self.memory_cache.is_empty() {
            if let Some(mr_state) = media_remote_reader::read_state() {
                let current_track = mr_state.track_name.clone();
                let track_changed = current_track != self.last_api_track;

                if mr_state.is_playing {
                    // Try to enrich from memory cache
                    if let Some(ref title) = current_track {
                        if let Some(metadata) = self.memory_cache.lookup_by_name(title) {
                            let has_complete =
                                metadata.neural_effect.is_some() && metadata.image_url.is_some();

                            if !track_changed && has_complete {
                                // Fast path: same track, complete data — no I/O needed
                                debug!(
                                    "Fast path: '{}' fully cached in memory, skipping disk I/O",
                                    title
                                );
                                state.is_playing = true;
                                state.track_name = Some(metadata.name.clone());
                                state.genre = metadata.genre.clone().or(state.genre);
                                state.neural_effect =
                                    metadata.neural_effect.clone().or(state.neural_effect);
                                state.mental_state_or_mode(metadata);
                                state.activity = metadata.activity.clone().or(state.activity);
                                state.image_url = metadata.image_url.clone().or(state.image_url);
                                return Ok(state);
                            }
                        }
                    }
                } else if !track_changed && self.last_api_track.is_some() {
                    // MediaRemote says not playing, same track context — quick not-playing
                    debug!("Fast path: not playing");
                    return Ok(state);
                }
            }
        }

        // 3. Full path: read disk cache + lsof (needed for first detection or incomplete data)
        let mut combined_cache = self.memory_cache.clone();

        if let Ok(disk_cache) = api_cache_reader::read_api_cache(&self.app_support_path) {
            combined_cache.merge(&disk_cache);
        }

        if !combined_cache.is_empty() {
            debug!(
                "Combined cache: {} tracks available (Memory: {}, Total unique: {})",
                combined_cache.len(),
                self.memory_cache.len(),
                combined_cache.len()
            );
        }

        // 4. Cache reader — detect what's currently playing via lsof
        let cache_state =
            match cache_reader::read_state(&self.app_support_path, Some(&mut combined_cache)) {
                Ok(s) => s,
                Err(e) => {
                    debug!("Cache reader error: {}", e);
                    BrainFmState::new()
                }
            };

        // 5. Determine if playing — lsof is primary, MediaRemote is fallback
        let (is_playing, current_track_key, detection_source) = if cache_state.is_playing {
            let track_key = cache_state.track_name.clone();
            (true, track_key, "lsof")
        } else if let Some(mr_state) = media_remote_reader::read_state() {
            if mr_state.is_playing {
                debug!("MediaRemote: Brain.fm is playing (lsof missed it)");
                let track_key = mr_state.track_name.clone();
                (true, track_key, "MediaRemote")
            } else {
                (false, None, "none")
            }
        } else {
            (false, None, "none")
        };

        if !is_playing {
            state = Self::merge_state(state, cache_state);
            return Ok(state);
        }

        // 6. Decide whether to call the Direct API:
        //    - ALWAYS on track change (new song needs fresh metadata)
        //    - Periodically every N cycles ONLY if metadata is incomplete
        self.api_refresh_counter += 1;
        let track_changed = current_track_key != self.last_api_track;

        // Check if current data is incomplete (missing key fields)
        let has_complete_metadata = cache_state.track_name.is_some()
            && cache_state.neural_effect.is_some()
            && cache_state.image_url.is_some();
        let periodic_refresh =
            !has_complete_metadata && self.api_refresh_counter >= API_REFRESH_INTERVAL;

        let should_call_api = track_changed || periodic_refresh;

        if should_call_api {
            if track_changed {
                debug!(
                    "Track changed ({:?} → {:?}), calling API for fresh metadata [detected by {}]",
                    self.last_api_track, current_track_key, detection_source
                );
            } else {
                debug!(
                    "Incomplete metadata, periodic API refresh (cycle {}) [detected by {}]",
                    self.api_refresh_counter, detection_source
                );
            }

            match api_client::fetch_recent_tracks(&self.app_support_path) {
                Ok(Some(api_data)) if !api_data.is_empty() => {
                    debug!("Direct API: {} tracks loaded", api_data.len());

                    // Update memory cache with fresh data
                    self.memory_cache.merge(&api_data);
                    combined_cache.merge(&api_data);
                    self.api_refresh_counter = 0;
                    self.last_api_track = current_track_key.clone();
                }
                Ok(Some(_)) => {
                    debug!("API returned empty result");
                }
                Ok(None) => {
                    warn!("API unavailable (token expired or not found), using cached data");
                }
                Err(e) => {
                    warn!("API error: {}, using cached data", e);
                }
            }
        }

        // 7. Enrich track data depending on detection source
        if detection_source == "lsof" {
            // Re-run cache reader with (potentially) API-enriched combined cache
            if let Ok(enriched_state) =
                cache_reader::read_state(&self.app_support_path, Some(&mut combined_cache))
            {
                state = Self::merge_state(state, enriched_state);
            } else {
                state = Self::merge_state(state, cache_state);
            }
        } else {
            // MediaRemote detected — enrich track name via cache lookup
            state.is_playing = true;
            if let Some(ref title) = current_track_key {
                if let Some(metadata) = combined_cache.lookup_by_name(title) {
                    debug!("MediaRemote: enriched '{}' from cache/API", title);
                    state.track_name = Some(metadata.name.clone());
                    state.genre = metadata.genre.clone().or(state.genre);
                    state.neural_effect = metadata.neural_effect.clone().or(state.neural_effect);
                    state.mental_state_or_mode(metadata);
                    state.activity = metadata.activity.clone().or(state.activity);
                    state.image_url = metadata.image_url.clone().or(state.image_url);
                } else {
                    debug!(
                        "MediaRemote: no cache/API match for '{}', using raw title",
                        title
                    );
                    state.track_name = Some(title.clone());
                }
            }
        }

        Ok(state)
    }

    /// Read from LevelDB local storage
    fn read_from_leveldb(&self) -> Result<BrainFmState> {
        leveldb_reader::read_state(&self.app_support_path)
    }

    /// Merge two states, preferring non-None values from the overlay state.
    ///
    /// For `is_playing`: overlay always wins (cache reader is authoritative for play/pause).
    fn merge_state(base: BrainFmState, overlay: BrainFmState) -> BrainFmState {
        BrainFmState {
            mode: overlay.mode.or(base.mode),
            is_playing: overlay.is_playing,
            track_name: overlay.track_name.or(base.track_name),
            neural_effect: overlay.neural_effect.or(base.neural_effect),
            genre: overlay.genre.or(base.genre),
            activity: overlay.activity.or(base.activity),
            image_url: overlay.image_url.or(base.image_url),
            session_state: overlay.session_state.or(base.session_state),
            session_time: overlay.session_time.or(base.session_time),
            infinite_play: overlay.infinite_play || base.infinite_play,
            adhd_mode: overlay.adhd_mode || base.adhd_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_state_option_overlay_wins() {
        let base = BrainFmState {
            mode: Some("Focus".into()),
            track_name: Some("Base Track".into()),
            ..Default::default()
        };
        let overlay = BrainFmState {
            mode: Some("Sleep".into()),
            ..Default::default()
        };
        let merged = BrainFmReader::merge_state(base, overlay);
        assert_eq!(merged.mode, Some("Sleep".into()));
        assert_eq!(merged.track_name, Some("Base Track".into()));
    }

    #[test]
    fn test_merge_state_is_playing_from_overlay() {
        let base = BrainFmState {
            is_playing: true,
            ..Default::default()
        };
        let overlay = BrainFmState {
            is_playing: false,
            ..Default::default()
        };
        let merged = BrainFmReader::merge_state(base, overlay);
        assert!(!merged.is_playing); // overlay wins even if false
    }

    #[test]
    fn test_merge_state_bool_or() {
        let base = BrainFmState {
            adhd_mode: true,
            ..Default::default()
        };
        let overlay = BrainFmState {
            infinite_play: true,
            ..Default::default()
        };
        let merged = BrainFmReader::merge_state(base, overlay);
        assert!(merged.adhd_mode); // base true || overlay false
        assert!(merged.infinite_play); // base false || overlay true
    }

    #[test]
    fn test_merge_state_both_none() {
        let base = BrainFmState::new();
        let overlay = BrainFmState::new();
        let merged = BrainFmReader::merge_state(base, overlay);
        assert!(merged.mode.is_none());
        assert!(merged.track_name.is_none());
    }
}
