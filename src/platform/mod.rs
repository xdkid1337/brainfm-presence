//! Platform abstraction layer
//!
//! This module provides platform-specific implementations for:
//! - Finding Brain.fm data directories
//! - Detecting if Brain.fm is running
//! - Loading platform-appropriate icons

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;
use std::path::PathBuf;

/// Platform-specific operations
pub trait Platform {
    /// Get the Brain.fm application support directory
    fn get_brainfm_data_dir() -> Result<PathBuf>;

    /// Check if Brain.fm is currently running
    fn is_brainfm_running() -> bool;

    /// Get the platform name for logging
    fn name() -> &'static str;
}

/// Get the current platform implementation
#[cfg(target_os = "macos")]
pub use macos::MacOSPlatform as CurrentPlatform;

#[cfg(target_os = "windows")]
pub use windows::WindowsPlatform as CurrentPlatform;

/// Get the Brain.fm data directory for the current platform
pub fn get_brainfm_data_dir() -> Result<PathBuf> {
    CurrentPlatform::get_brainfm_data_dir()
}

/// Check if Brain.fm is running on the current platform
pub fn is_brainfm_running() -> bool {
    CurrentPlatform::is_brainfm_running()
}
