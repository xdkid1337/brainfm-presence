//! Brain.fm information reader
//! 
//! This module provides functionality to read the current state of Brain.fm app
//! including the active mode (Deep Work, Light Work, etc.), current track, and session time.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod cache_reader;
pub mod leveldb_reader;
pub mod platform;
pub mod tray;

/// Represents the current state of Brain.fm playback
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainFmState {
    /// Current mode (e.g., "Deep Work", "Light Work", "Motivation", "Sleep", "Relax")
    pub mode: Option<String>,
    
    /// Whether currently playing
    pub is_playing: bool,
    
    /// Current track name
    pub track_name: Option<String>,
    
    /// Neural effect level (e.g., "High Neural Effect", "Medium Neural Effect")
    pub neural_effect: Option<String>,
    
    /// Genre/category (e.g., "PIANO", "ELECTRONIC")
    pub genre: Option<String>,
    
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
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if Brain.fm is actively playing
    pub fn is_active(&self) -> bool {
        self.is_playing && self.mode.is_some()
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
    
    /// Get details string for Discord Rich Presence
    pub fn to_details_string(&self) -> Option<String> {
        let mut parts = Vec::new();
        
        if let Some(ref track) = self.track_name {
            parts.push(track.clone());
        }
        
        if let Some(ref effect) = self.neural_effect {
            parts.push(effect.clone());
        }
        
        if let Some(ref genre) = self.genre {
            parts.push(genre.clone());
        }
        
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" • "))
        }
    }
}

/// Main reader that combines multiple data sources
pub struct BrainFmReader {
    /// Path to Brain.fm app support directory
    app_support_path: PathBuf,
}

impl BrainFmReader {
    /// Create a new reader
    pub fn new() -> Result<Self> {
        let app_support_path = platform::get_brainfm_data_dir()?;
        Ok(Self { app_support_path })
    }
    
    /// Check if Brain.fm is running
    pub fn is_running(&self) -> bool {
        platform::is_brainfm_running()
    }
    
    /// Read current state using all available methods
    pub fn read_state(&self) -> Result<BrainFmState> {
        let mut state = BrainFmState::new();
        
        // Check if app is running
        if !self.is_running() {
            return Ok(state);
        }
        
        // Don't hardcode is_playing here — let the scrapers determine it.
        // The cache reader uses lsof to detect play/pause state.
        
        // Read sources in order of reliability:
        // - LevelDB: Baseline data (may be stale for mode)
        // - Cache: Best for track info and mode from audio URLs
        
        // 1. LevelDB (baseline data, may be stale)
        if let Ok(leveldb_state) = self.read_from_leveldb() {
            state = self.merge_state(state, leveldb_state);
        }
        
        // 2. Cache (HIGHEST PRIORITY - has current track and mode from URL)
        // Also provides authoritative play/pause state via lsof
        if let Ok(cache_state) = self.read_from_cache() {
            state = self.merge_state(state, cache_state);
        }
        
        Ok(state)
    }
    
    /// Read from LevelDB local storage
    fn read_from_leveldb(&self) -> Result<BrainFmState> {
        leveldb_reader::read_state(&self.app_support_path)
    }
    
    /// Read from Cache
    fn read_from_cache(&self) -> Result<BrainFmState> {
        cache_reader::read_state(&self.app_support_path)
    }
    
    /// Merge two states, preferring non-None values from the overlay state.
    /// For is_playing: overlay wins (cache reader is authoritative for play/pause).
    fn merge_state(&self, base: BrainFmState, overlay: BrainFmState) -> BrainFmState {
        BrainFmState {
            mode: overlay.mode.or(base.mode),
            // Overlay (higher priority) determines play/pause state.
            // Cache reader sets is_playing based on lsof (true = playing, false = paused).
            is_playing: overlay.is_playing,
            track_name: overlay.track_name.or(base.track_name),
            neural_effect: overlay.neural_effect.or(base.neural_effect),
            genre: overlay.genre.or(base.genre),
            session_state: overlay.session_state.or(base.session_state),
            session_time: overlay.session_time.or(base.session_time),
            infinite_play: overlay.infinite_play || base.infinite_play,
            adhd_mode: overlay.adhd_mode || base.adhd_mode,
        }
    }
}

impl Default for BrainFmReader {
    fn default() -> Self {
        Self::new().expect("Failed to create BrainFmReader")
    }
}
