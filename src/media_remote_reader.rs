//! MediaRemote reader for Brain.fm
//!
//! Uses macOS MediaRemote framework (via `mediaremote-rs`) to detect
//! whether Brain.fm is currently playing audio. This provides a reliable
//! `is_playing` signal that doesn't depend on `lsof` cache file detection,
//! which can fail during long playback sessions.
//!
//! # How it works
//!
//! Electron apps (like Brain.fm) integrate with macOS Media Session API,
//! which exposes now-playing info through the private MediaRemote framework.
//! The `mediaremote-rs` crate handles macOS 15.4+ restrictions automatically
//! via a dual-process Perl adapter architecture.
//!
//! # Bundle ID
//!
//! Brain.fm's Electron app registers as `com.electron.brain.fm`.

use log::debug;

/// Brain.fm's macOS bundle identifier
const BRAINFM_BUNDLE_ID: &str = "com.electron.brain.fm";

/// Simplified state from MediaRemote, filtered for Brain.fm
#[derive(Debug, Clone)]
pub struct MediaRemoteState {
    /// Whether Brain.fm is actively playing audio
    pub is_playing: bool,

    /// Track title as reported by Brain.fm to macOS (e.g., "Nocturne")
    pub track_name: Option<String>,

    /// Elapsed playback time in seconds
    pub elapsed_secs: Option<f64>,

    /// Total duration in seconds
    pub duration_secs: Option<f64>,
}

/// Read Brain.fm playback state from macOS MediaRemote framework.
///
/// Returns `Some(state)` if Brain.fm is the current Now Playing app,
/// `None` if MediaRemote is inaccessible or another app is playing.
#[cfg(target_os = "macos")]
pub fn read_state() -> Option<MediaRemoteState> {
    let info = mediaremote_rs::get_now_playing()?;

    // Only care about Brain.fm
    if info.bundle_identifier != BRAINFM_BUNDLE_ID {
        debug!(
            "MediaRemote: active app is '{}', not Brain.fm",
            info.bundle_identifier
        );
        return None;
    }

    let track_name = if info.title.is_empty() {
        None
    } else {
        Some(info.title)
    };

    debug!(
        "MediaRemote: Brain.fm playing={}, track={:?}",
        info.playing,
        track_name
    );

    Some(MediaRemoteState {
        is_playing: info.playing,
        track_name,
        elapsed_secs: info.elapsed_time,
        duration_secs: info.duration,
    })
}

/// Stub for non-macOS platforms — always returns None.
#[cfg(not(target_os = "macos"))]
pub fn read_state() -> Option<MediaRemoteState> {
    None
}
