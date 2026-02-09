//! Shared utility functions for Brain.fm Presence
//!
//! Consolidates duplicated logic from across the codebase into a single module.
//! See: design.md §6 "Shared Utility Module"

use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Default timeout for external commands (lsof, pgrep, etc.)
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Shared regex and constants
// ---------------------------------------------------------------------------

/// Regex for extracting .mp3 filename from a URL.
///
/// Shared between `cache_reader` and `leveldb_reader`.
pub static MP3_FILENAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/([^/?]+)\.mp3").unwrap());

/// Known Brain.fm genres for heuristic filename parsing (lowercase).
///
/// Union of genres used across `cache_reader` and `leveldb_reader`.
pub const KNOWN_GENRES: &[&str] = &[
    "piano",
    "electronic",
    "lofi",
    "ambient",
    "nature",
    "atmospheric",
    "grooves",
    "cinematic",
    "classical",
    "acoustic",
    "drone",
    "postrock",
    "chimes",
    "rain",
    "forest",
    "thunder",
    "beach",
    "night",
    "river",
    "wind",
    "underwater",
];

// ---------------------------------------------------------------------------
// URL decoding
// ---------------------------------------------------------------------------

/// Simple URL decode for common percent-encoded patterns.
///
/// **Not general-purpose.** Only decodes a small, hardcoded set of
/// percent-encoded sequences (`%20`, `%2F`, `%3A`, `%3D`, `%26`, `%2B`)
/// commonly found in Brain.fm audio URLs. Does not handle arbitrary
/// percent-encoding, multi-byte UTF-8 sequences, or `+` as space.
///
/// Shared between `cache_reader` and `api_cache_reader`.
pub fn url_decode(s: &str) -> String {
    s.replace("%20", " ")
        .replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%3D", "=")
        .replace("%26", "&")
        .replace("%2B", "+")
}

// ---------------------------------------------------------------------------
// Unicode-safe string truncation
// ---------------------------------------------------------------------------

/// Truncate a string to at most `max_chars` Unicode characters.
///
/// If truncated, appends "..." so the total character count is ≤ `max_chars`.
/// Never panics on multi-byte characters (unlike byte-index slicing).
pub fn truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ---------------------------------------------------------------------------
// Mode pattern matching
// ---------------------------------------------------------------------------

/// Known Brain.fm mode patterns for matching against LevelDB/URL data.
///
/// Each tuple is (pattern_to_match, canonical_display_name).
pub const MODE_PATTERNS: &[(&str, &str)] = &[
    ("Deep Work", "Deep Work"),
    ("Light Work", "Light Work"),
    ("Motivation", "Motivation"),
    ("Focus", "Focus"),
    ("Sleep", "Sleep"),
    ("Relax", "Relax"),
    ("Meditate", "Meditate"),
    ("Recharge", "Recharge"),
];

// ---------------------------------------------------------------------------
// Genre icon mapping
// ---------------------------------------------------------------------------

/// Map a genre string to its Brain.fm CDN icon URL (case-insensitive).
///
/// Falls back to the electronic icon for unknown genres.
pub fn genre_icon_url(genre: &str) -> &'static str {
    match genre.to_lowercase().as_str() {
        // Base genres
        "lofi" => "https://cdn.brain.fm/icons/lofi.png",
        "piano" => "https://cdn.brain.fm/icons/piano.png",
        "electronic" => "https://cdn.brain.fm/icons/electronic.png",
        "grooves" => "https://cdn.brain.fm/icons/grooves.png",
        "atmospheric" => "https://cdn.brain.fm/icons/atmospheric.png",
        "cinematic" => "https://cdn.brain.fm/icons/cinematic.png",
        "classical" => "https://cdn.brain.fm/icons/classical.png",
        "acoustic" => "https://cdn.brain.fm/icons/acoustic.png",
        "drone" => "https://cdn.brain.fm/icons/drone.png",
        "post rock" => "https://cdn.brain.fm/icons/post_rock.png",
        // Nature / atmosphere genres
        "rain" => "https://cdn.brain.fm/icons/rain.png",
        "forest" => "https://cdn.brain.fm/icons/forest.png",
        "beach" => "https://cdn.brain.fm/icons/beach.png",
        "night" | "nightsounds" => "https://cdn.brain.fm/icons/night.png",
        "thunder" => "https://cdn.brain.fm/icons/thunder.png",
        "wind" => "https://cdn.brain.fm/icons/wind.png",
        "river" => "https://cdn.brain.fm/icons/river.png",
        "rainforest" => "https://cdn.brain.fm/icons/rainforest.png",
        "underwater" => "https://cdn.brain.fm/icons/underwater.png",
        "chimes & bowls" | "chimes and bowls" => "https://cdn.brain.fm/icons/chimes.png",
        _ => "https://cdn.brain.fm/icons/electronic.png",
    }
}

// ---------------------------------------------------------------------------
// Native LevelDB string extraction
// ---------------------------------------------------------------------------

/// Read all printable string content from LevelDB files using native Rust I/O.
///
/// Replaces `Command::new("sh").args(["-c", "strings ..."])` — uses
/// `std::fs::read_dir` + printable ASCII extraction. Runs of ≥ 4 printable
/// bytes are collected as individual lines.
pub fn read_leveldb_strings(leveldb_path: &Path) -> Result<String> {
    let mut content = String::new();

    for entry in std::fs::read_dir(leveldb_path)
        .with_context(|| format!("Failed to read LevelDB directory: {leveldb_path:?}"))?
    {
        let entry = entry?;
        let path = entry.path();

        match path.extension().and_then(|e| e.to_str()) {
            Some("ldb" | "log") => {
                if let Ok(bytes) = std::fs::read(&path) {
                    extract_printable_strings(&bytes, &mut content);
                }
            }
            _ => {}
        }
    }

    Ok(content)
}

/// Extract runs of ≥ 4 printable ASCII bytes from raw data (mimics `strings`).
fn extract_printable_strings(bytes: &[u8], out: &mut String) {
    let mut current = Vec::new();
    for &b in bytes {
        if b.is_ascii_graphic() || b == b' ' {
            current.push(b);
        } else {
            if current.len() >= 4 {
                if let Ok(s) = std::str::from_utf8(&current) {
                    out.push_str(s);
                    out.push('\n');
                }
            }
            current.clear();
        }
    }
    // Flush trailing run
    if current.len() >= 4 {
        if let Ok(s) = std::str::from_utf8(&current) {
            out.push_str(s);
            out.push('\n');
        }
    }
}

// ---------------------------------------------------------------------------
// Command execution with timeout
// ---------------------------------------------------------------------------

/// Run a command with a timeout. Kills the child if it exceeds the deadline.
///
/// Drains stdout/stderr in background threads to avoid pipe-buffer deadlocks
/// (a common issue when the child's output exceeds the OS pipe capacity).
pub fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn command")?;

    // Take ownership of pipes and drain them in background threads
    // to prevent the child from blocking on a full pipe buffer.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_handle {
            std::io::Read::read_to_end(&mut out, &mut buf).ok();
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_handle {
            std::io::Read::read_to_end(&mut err, &mut buf).ok();
        }
        buf
    });

    // Poll for exit with timeout
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    child.kill().ok();
                    child.wait().ok();
                    anyhow::bail!("Command timed out after {timeout:?}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- url_decode --

    #[test]
    fn test_url_decode_spaces() {
        assert_eq!(url_decode("Hello%20World"), "Hello World");
    }

    #[test]
    fn test_url_decode_no_encoding() {
        assert_eq!(url_decode("no_encoding"), "no_encoding");
    }

    #[test]
    fn test_url_decode_all_patterns() {
        assert_eq!(url_decode("a%2Fb%3Ac%3Dd%26e%2Bf"), "a/b:c=d&e+f");
    }

    // -- truncate --

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("Hi", 10), "Hi");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("Hello", 5), "Hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("Hello, World!", 10), "Hello, ...");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn test_truncate_unicode() {
        // Multi-byte: "日本語テスト" (6 chars, each 3 bytes)
        let s = "日本語テスト"; // 6 chars
        let result = truncate(s, 5);
        assert_eq!(result, "日本...");
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_emoji() {
        let s = "🧠🎵🎶🎧🎤🎹";
        let result = truncate(s, 5);
        assert_eq!(result, "🧠🎵...");
    }

    // -- genre_icon_url --

    #[test]
    fn test_genre_icon_url_known() {
        assert_eq!(
            genre_icon_url("Piano"),
            "https://cdn.brain.fm/icons/piano.png"
        );
    }

    #[test]
    fn test_genre_icon_url_case_insensitive() {
        assert_eq!(genre_icon_url("PIANO"), genre_icon_url("piano"));
        assert_eq!(genre_icon_url("LoFi"), genre_icon_url("lofi"));
        assert_eq!(genre_icon_url("Electronic"), genre_icon_url("electronic"));
    }

    #[test]
    fn test_genre_icon_url_unknown_fallback() {
        assert_eq!(
            genre_icon_url("UnknownGenre"),
            "https://cdn.brain.fm/icons/electronic.png"
        );
    }

    // -- read_leveldb_strings --

    #[test]
    fn test_extract_printable_strings() {
        let mut out = String::new();
        // "Hello" (5 bytes) + null + "ab" (2 bytes, too short) + null + "Test" (4 bytes)
        let data = b"Hello\x00ab\x00Test";
        extract_printable_strings(data, &mut out);
        assert!(out.contains("Hello"));
        assert!(out.contains("Test"));
        assert!(!out.contains("ab")); // too short (< 4)
    }

    // -- run_command_with_timeout --

    #[test]
    fn test_command_with_timeout_success() {
        let output =
            run_command_with_timeout(Command::new("echo").arg("hello"), Duration::from_secs(5))
                .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[test]
    fn test_command_with_timeout_times_out() {
        let result =
            run_command_with_timeout(Command::new("sleep").arg("10"), Duration::from_secs(1));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timed out"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_truncate_never_panics(s in ".*", max in 0usize..200) {
            let _ = truncate(&s, max);
        }

        #[test]
        fn prop_truncate_respects_max_chars(s in ".{0,100}", max in 3usize..100) {
            let result = truncate(&s, max);
            prop_assert!(result.chars().count() <= max);
        }

        #[test]
        fn prop_truncate_short_identity(s in ".{0,10}") {
            let result = truncate(&s, 100);
            prop_assert_eq!(result, s);
        }

        #[test]
        fn prop_genre_icon_url_returns_url(genre in "[a-zA-Z]{1,20}") {
            let url = genre_icon_url(&genre);
            prop_assert!(url.starts_with("https://"));
            prop_assert!(url.ends_with(".png"));
        }

        #[test]
        fn prop_url_decode_idempotent_on_plain(s in "[a-zA-Z0-9_.-]{0,50}") {
            // Plain ASCII without percent-encoded chars should pass through unchanged
            prop_assert_eq!(url_decode(&s), s);
        }
    }
}
