//! macOS platform implementation
//!
//! Provides macOS-specific functionality for Brain.fm presence detection.

use super::Platform;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

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
        let output = Command::new("pgrep")
            .args(["-x", "Brain.fm"])
            .output();
        
        matches!(output, Ok(o) if o.status.success())
    }
    
    fn name() -> &'static str {
        "macOS"
    }
}
