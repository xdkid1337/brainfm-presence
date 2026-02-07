//! macOS platform implementation
//!
//! Provides macOS-specific functionality for Brain.fm presence detection.

use super::Platform;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use crate::util;

/// macOS platform implementation
pub struct MacOSPlatform;

impl Platform for MacOSPlatform {
    fn get_brainfm_data_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        let path = home
            .join("Library")
            .join("Application Support")
            .join("Brain.fm");
        
        if !path.exists() {
            anyhow::bail!(
                "Brain.fm app support directory not found at {:?}. \
                 Make sure Brain.fm is installed and has been run at least once.",
                path
            );
        }
        
        Ok(path)
    }
    
    fn is_brainfm_running() -> bool {
        util::run_command_with_timeout(
            Command::new("pgrep").args(["-x", "Brain.fm"]),
            util::DEFAULT_COMMAND_TIMEOUT,
        )
        .map(|output| output.status.success())
        .unwrap_or(false)
    }
    
    fn name() -> &'static str {
        "macOS"
    }
}
