//! Windows platform implementation (stub)
//!
//! This module provides stub implementations for Windows.
//! Full Windows support requires testing with Brain.fm on Windows
//! to determine the correct data directory paths.

use super::Platform;
use anyhow::Result;
use std::path::PathBuf;

/// Windows platform implementation (stub)
pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn get_brainfm_data_dir() -> Result<PathBuf> {
        // Brain.fm on Windows is likely an Electron app, which typically stores
        // data in one of these locations:
        // - %APPDATA%\Brain.fm
        // - %LOCALAPPDATA%\Brain.fm
        // - %APPDATA%\brain-fm (lowercase)

        // Try common locations
        if let Some(appdata) = dirs::data_dir() {
            let path = appdata.join("Brain.fm");
            if path.exists() {
                return Ok(path);
            }
        }

        if let Some(local_appdata) = dirs::data_local_dir() {
            let path = local_appdata.join("Brain.fm");
            if path.exists() {
                return Ok(path);
            }
        }

        anyhow::bail!(
            "Brain.fm data directory not found on Windows. \
             This platform is not yet fully supported. \
             Please open an issue with your Brain.fm installation path."
        )
    }

    fn is_brainfm_running() -> bool {
        // On Windows, we would use the Windows API to check for the process
        // For now, return false as a stub
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            // Use run_command_with_timeout to prevent indefinite hangs
            if let Ok(output) = crate::util::run_command_with_timeout(
                Command::new("tasklist").args(["/FI", "IMAGENAME eq Brain.fm.exe"]),
                crate::util::DEFAULT_COMMAND_TIMEOUT,
            ) {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.contains("Brain.fm.exe");
            }
        }

        false
    }

    fn name() -> &'static str {
        "Windows"
    }
}
