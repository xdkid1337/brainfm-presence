//! Direct API client for Brain.fm
//!
//! Reads the JWT access token from LevelDB (`persist:auth`) and calls
//! `api.brain.fm` to fetch the user's recent tracks with full metadata.
//!
//! The Brain.fm Electron app refreshes the JWT every ~5 minutes.
//! If the token is expired, we skip the API call and let the caller
//! fall back to cache scraping.

use anyhow::{Context, Result};
use base64::prelude::*;
use log::debug;
use regex::Regex;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api_cache_reader::{ApiCacheData, parse_servings_json};

/// Auth credentials extracted from LevelDB
struct AuthInfo {
    token: String,
    user_id: String,
}

/// Fetch recent tracks directly from the Brain.fm API.
///
/// Returns `Ok(Some(data))` on success, `Ok(None)` if the token is expired
/// or unavailable, and `Err` only on unexpected failures.
pub fn fetch_recent_tracks(app_support_path: &Path) -> Result<Option<ApiCacheData>> {
    // 1. Extract auth from LevelDB
    let auth = match extract_auth(app_support_path) {
        Ok(Some(a)) => a,
        Ok(None) => {
            debug!("No auth token found in LevelDB");
            return Ok(None);
        }
        Err(e) => {
            debug!("Failed to extract auth: {}", e);
            return Ok(None);
        }
    };

    // 2. Check if token is expired
    if is_token_expired(&auth.token) {
        debug!("Access token is expired, skipping API call");
        return Ok(None);
    }

    // 3. Call the API
    let url = format!(
        "https://api.brain.fm/v3/users/{}/servings/recent",
        auth.user_id
    );

    debug!("Fetching recent tracks from API: {}", url);

    let mut response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {}", auth.token))
        .header("Accept", "application/json")
        .call()
        .context("API request failed")?;

    let body = response.body_mut().read_to_string()?;

    // 4. Parse using the same logic as the cache reader
    let data = parse_servings_json(&body)?;

    debug!("API returned {} tracks", data.len());

    Ok(Some(data))
}

/// Extract JWT access token and user ID from LevelDB's `persist:auth`.
///
/// The Brain.fm Electron app stores its Redux auth state in LevelDB with the key
/// `persist:auth`. The value contains a JSON object with `token` and `userId` fields.
fn extract_auth(app_support_path: &Path) -> Result<Option<AuthInfo>> {
    let leveldb_path = app_support_path.join("Local Storage").join("leveldb");

    if !leveldb_path.exists() {
        return Ok(None);
    }

    // Read strings from LevelDB files (same approach as leveldb_reader)
    let output = Command::new("sh")
        .args([
            "-c",
            &format!(
                "strings {:?}/*.ldb {:?}/*.log 2>/dev/null",
                leveldb_path, leveldb_path
            ),
        ])
        .output()
        .context("Failed to run strings command")?;

    let content = String::from_utf8_lossy(&output.stdout);

    // Find JWT tokens — there may be multiple (old expired ones linger in .ldb files).
    // We want the freshest valid one.
    let jwt_re = Regex::new(r"eyJ[A-Za-z0-9_\-]+\.eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap();

    // Find user ID from the persist:auth JSON (pattern: "userId":"\"xxx\"")
    let uid_re = Regex::new(r#""userId":\s*"\\?"([A-Za-z0-9_\-]+)\\?""#).unwrap();

    // Collect all JWT tokens, prefer the last non-expired one (most recent in file order)
    let all_tokens: Vec<&str> = jwt_re.find_iter(&content).map(|m| m.as_str()).collect();
    let token = all_tokens
        .iter()
        .rev() // Check newest first (last in file = most recent write)
        .find(|t| !is_token_expired(t))
        .or_else(|| all_tokens.last()) // If all expired, use the most recent anyway
        .map(|t| t.to_string());

    let user_id = uid_re
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    match (token, user_id) {
        (Some(t), Some(u)) => Ok(Some(AuthInfo {
            token: t,
            user_id: u,
        })),
        _ => Ok(None),
    }
}

/// Check if a JWT token is expired by decoding its payload.
///
/// Returns `true` if expired or if the token can't be decoded.
fn is_token_expired(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return true;
    }

    // Decode the payload (second part) with URL-safe base64
    let payload_bytes = match BASE64_URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(b) => b,
        Err(_) => return true,
    };

    let payload_str = match std::str::from_utf8(&payload_bytes) {
        Ok(s) => s,
        Err(_) => return true,
    };

    // Extract "exp" field — we do a simple regex to avoid pulling in serde_json
    // just for this one check (the payload is always {"...","exp":1234567890.4,...})
    let exp_re = Regex::new(r#""exp"\s*:\s*([0-9]+(?:\.[0-9]+)?)"#).unwrap();
    let exp = match exp_re.captures(payload_str) {
        Some(c) => match c[1].parse::<f64>() {
            Ok(v) => v,
            Err(_) => return true,
        },
        None => return true,
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    now > exp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_token_expired_with_past_token() {
        // Create a fake JWT with exp in the past (exp: 1000000000 = Sep 2001)
        let header = BASE64_URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload =
            BASE64_URL_SAFE_NO_PAD.encode(r#"{"_id":"test","exp":1000000000,"iat":999999700}"#);
        let token = format!("{}.{}.fakesig", header, payload);
        assert!(is_token_expired(&token));
    }

    #[test]
    fn test_is_token_expired_with_future_token() {
        // Create a fake JWT with exp far in the future (exp: 9999999999 = Nov 2286)
        let header = BASE64_URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload =
            BASE64_URL_SAFE_NO_PAD.encode(r#"{"_id":"test","exp":9999999999,"iat":9999999699}"#);
        let token = format!("{}.{}.fakesig", header, payload);
        assert!(!is_token_expired(&token));
    }

    #[test]
    fn test_is_token_expired_with_garbage() {
        assert!(is_token_expired("not-a-jwt"));
        assert!(is_token_expired(""));
        assert!(is_token_expired("a.b.c"));
    }
}
