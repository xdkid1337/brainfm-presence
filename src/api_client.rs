//! Direct API client for Brain.fm
//!
//! Reads the JWT access token from LevelDB (`persist:auth`) and calls
//! `api.brain.fm` to fetch the user's recent tracks with full metadata.
//!
//! The Brain.fm Electron app refreshes the JWT every ~5 minutes.
//! If the token is expired, we skip the API call and let the caller
//! fall back to cache scraping.

use anyhow::Result;
use base64::prelude::*;
use log::{debug, warn};
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::api_cache_reader::{parse_servings_json, ApiCacheData};

/// Regex for matching JWT tokens
static JWT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"eyJ[A-Za-z0-9_\-]+\.eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap()
});

/// Regex for extracting user ID from persist:auth
static USER_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""userId":\s*"\\?"([A-Za-z0-9_\-]+)\\?""#).unwrap());

/// Regex for extracting exp claim from JWT payload
static EXP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""exp"\s*:\s*([0-9]+(?:\.[0-9]+)?)"#).unwrap());

/// Shared HTTP agent with connection pooling and timeouts
static HTTP_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent()
});

/// Safety buffer for token expiry check (seconds).
/// Tokens expiring within this window are treated as expired to avoid race
/// conditions between local check and server-side validation.
const TOKEN_EXPIRY_BUFFER_SECS: f64 = 30.0;

/// Retry delays for API calls (in seconds): immediate, 2s, 5s
const RETRY_DELAYS: &[u64] = &[0, 2, 5];

/// Auth credentials extracted from LevelDB
struct AuthInfo {
    token: String,
    user_id: String,
}

/// Fetch recent tracks directly from the Brain.fm API.
///
/// Returns `Ok(Some(data))` on success, `Ok(None)` if the token is expired
/// or unavailable, and `Err` only on unexpected failures.
///
/// Retries up to 3 times with delays `[0s, 2s, 5s]`. On HTTP 401, re-reads
/// the JWT from LevelDB before retrying (the Electron app may have refreshed it).
pub fn fetch_recent_tracks(app_support_path: &Path) -> Result<Option<ApiCacheData>> {
    let max_attempts = RETRY_DELAYS.len();

    for attempt in 0..max_attempts {
        // Apply delay (0 on first attempt)
        let delay = RETRY_DELAYS[attempt];
        if delay > 0 {
            debug!(
                "API retry {}/{}: waiting {}s before next attempt",
                attempt + 1,
                max_attempts,
                delay
            );
            std::thread::sleep(Duration::from_secs(delay));
        }

        // 1. Extract auth from LevelDB (re-read on each retry to pick up refreshed tokens)
        let auth = match extract_auth(app_support_path) {
            Ok(Some(a)) => a,
            Ok(None) => {
                debug!(
                    "No auth token found in LevelDB (attempt {}/{})",
                    attempt + 1,
                    max_attempts
                );
                // No token at all — no point retrying
                return Ok(None);
            }
            Err(e) => {
                warn!(
                    "Failed to extract auth (attempt {}/{}): {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                continue;
            }
        };

        // 2. Check if token is expired (with safety buffer)
        if is_token_expired(&auth.token) {
            debug!(
                "Access token is expired (attempt {}/{}), will retry to pick up refreshed token",
                attempt + 1,
                max_attempts
            );
            continue;
        }

        // 3. Call the API
        let url = format!(
            "https://api.brain.fm/v3/users/{}/servings/recent",
            auth.user_id
        );

        debug!(
            "Fetching recent tracks from API (attempt {}/{}): {}",
            attempt + 1,
            max_attempts,
            url
        );

        match HTTP_AGENT
            .get(&url)
            .header("Authorization", &format!("Bearer {}", auth.token))
            .header("Accept", "application/json")
            .call()
        {
            Ok(mut response) => {
                let body = response.body_mut().read_to_string()?;
                let data = parse_servings_json(&body)?;
                debug!("API returned {} tracks", data.len());
                return Ok(Some(data));
            }
            Err(ureq::Error::StatusCode(401)) => {
                warn!("API returned 401 Unauthorized (attempt {}/{}), token may have just expired — will re-read LevelDB", attempt + 1, max_attempts);
                // Loop continues → next iteration will re-read LevelDB for a fresh token
                continue;
            }
            Err(ureq::Error::StatusCode(code)) => {
                warn!(
                    "API returned HTTP {} (attempt {}/{})",
                    code,
                    attempt + 1,
                    max_attempts
                );
                continue;
            }
            Err(e) => {
                warn!(
                    "API request failed (attempt {}/{}): {}",
                    attempt + 1,
                    max_attempts,
                    e
                );
                continue;
            }
        }
    }

    debug!(
        "All {} API attempts exhausted, returning None",
        max_attempts
    );
    Ok(None)
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
    let leveldb_content = crate::util::read_leveldb_strings(&leveldb_path)?;
    let content = leveldb_content;

    // Collect all JWT tokens, prefer the last non-expired one (most recent in file order)
    let all_tokens: Vec<&str> = JWT_RE.find_iter(&content).map(|m| m.as_str()).collect();
    let token = all_tokens
        .iter()
        .rev() // Check newest first (last in file = most recent write)
        .find(|t| !is_token_expired(t))
        .or_else(|| all_tokens.last()) // If all expired, use the most recent anyway
        .map(|t| t.to_string());

    let user_id = USER_ID_RE
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
    let exp = match EXP_RE.captures(payload_str) {
        Some(c) => match c[1].parse::<f64>() {
            Ok(v) => v,
            Err(_) => return true,
        },
        None => return true,
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs_f64();

    // Add safety buffer to account for network latency between local check
    // and server-side validation
    now + TOKEN_EXPIRY_BUFFER_SECS > exp
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

    #[test]
    fn test_token_expiry_buffer_30s() {
        // Token expiring in 15 seconds should be considered expired (within 30s buffer)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp_soon = now + 15; // expires in 15s
        let header = BASE64_URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = BASE64_URL_SAFE_NO_PAD.encode(format!(
            r#"{{"_id":"test","exp":{},"iat":{}}}"#,
            exp_soon,
            exp_soon - 300
        ));
        let token = format!("{}.{}.fakesig", header, payload);
        assert!(
            is_token_expired(&token),
            "Token expiring in 15s should be treated as expired"
        );
    }

    #[test]
    fn test_token_valid_with_buffer() {
        // Token expiring in 60 seconds should still be valid (outside 30s buffer)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp_later = now + 60; // expires in 60s
        let header = BASE64_URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = BASE64_URL_SAFE_NO_PAD.encode(format!(
            r#"{{"_id":"test","exp":{},"iat":{}}}"#,
            exp_later,
            exp_later - 300
        ));
        let token = format!("{}.{}.fakesig", header, payload);
        assert!(
            !is_token_expired(&token),
            "Token expiring in 60s should still be valid"
        );
    }
}
