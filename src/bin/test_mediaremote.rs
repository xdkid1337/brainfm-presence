//! `MediaRemote` test binary
//!
//! Tests whether macOS `MediaRemote` framework can detect Brain.fm playback.
//! Run with: cargo run --bin brainfm-mediaremote-test
//!
//! Make sure Brain.fm is playing audio before running this.

use brainfm_presence::util::truncate;

fn main() {
    #[cfg(target_os = "macos")]
    macos_test();

    #[cfg(not(target_os = "macos"))]
    println!("MediaRemote is only available on macOS.");
}

#[cfg(target_os = "macos")]
fn macos_test() {
    use mediaremote_rs::{get_now_playing, is_playing, test_access};
    use std::time::Duration;

    println!("🔬 MediaRemote Framework Test for Brain.fm");
    println!("============================================\n");

    // Step 1: Check access
    println!("1️⃣  Testing MediaRemote access...");
    if test_access() {
        println!("   ✅ MediaRemote is accessible!\n");
    } else {
        println!("   ❌ MediaRemote access denied.");
        println!("   This may be a macOS permissions issue.");
        println!("   The library should handle macOS 15.4+ via Perl adapter.\n");
        // Continue anyway — test_access might be conservative
    }

    // Step 2: Check is_playing
    println!("2️⃣  Checking if any media is playing...");
    let playing = is_playing();
    println!("   is_playing() = {playing}\n");

    // Step 3: Get now playing info
    println!("3️⃣  Getting Now Playing info...");
    if let Some(info) = get_now_playing() {
        println!("   ✅ Got Now Playing data!\n");
        println!("   ┌─────────────────────────────────────────────┐");
        println!("   │ MediaRemote Now Playing Info                │");
        println!("   ├─────────────────────────────────────────────┤");
        println!("   │ Bundle ID:     {:30} │", info.bundle_identifier);
        println!("   │ Playing:       {:30} │", info.playing);
        println!("   │ Title:         {:30} │", truncate(&info.title, 30));
        println!("   │ Artist:        {:30} │", info.artist.as_deref().unwrap_or("(none)"));
        println!("   │ Album:         {:30} │", info.album.as_deref().unwrap_or("(none)"));
        if let Some(dur) = info.duration {
            println!("   │ Duration:      {dur:>27.1}s │");
        } else {
            println!("   │ Duration:      {:30} │", "(none)");
        }
        if let Some(elapsed) = info.elapsed_time {
            println!("   │ Elapsed:       {elapsed:>27.1}s │");
        } else {
            println!("   │ Elapsed:       {:30} │", "(none)");
        }
        if let Some(rate) = info.playback_rate {
            println!("   │ Playback Rate: {rate:30} │");
        }
        println!("   │ Has Artwork:   {:30} │", info.artwork_data.is_some());
        if let Some(ref mime) = info.artwork_mime_type {
            println!("   │ Artwork MIME:  {mime:30} │");
        }
        println!("   └─────────────────────────────────────────────┘");

        // Check if this is Brain.fm
        let is_brainfm = info.bundle_identifier.to_lowercase().contains("brain")
            || info.bundle_identifier.to_lowercase().contains("brainfm")
            || info.artist.as_deref().is_some_and(|a| a.to_lowercase().contains("brain"));

        println!();
        if is_brainfm {
            println!("   🧠 This IS Brain.fm! MediaRemote can detect it.");
            println!("   → bundle_identifier: {}", info.bundle_identifier);
            println!("   → We can use this for reliable is_playing detection.");
        } else {
            println!("   ⚠️  This doesn't appear to be Brain.fm.");
            println!("   → Detected app: {}", info.bundle_identifier);
            println!("   → Make sure Brain.fm is actively playing audio.");
            println!("   → Try pausing other media players first.");
        }

        // Raw JSON dump for debugging
        println!("\n4️⃣  Raw JSON (for debugging):");
        if let Ok(json) = serde_json::to_string_pretty(&info) {
            println!("{json}");
        }
    } else {
        println!("   ⚠️  No Now Playing info available.");
        println!("   → Make sure Brain.fm (or any media) is actively playing.");
        println!("   → The app must be producing audio for MediaRemote to detect it.");
    }

    // Step 4: Monitor for 15 seconds to see changes
    println!("\n5️⃣  Monitoring for 15 seconds (try play/pause in Brain.fm)...");
    let receiver = mediaremote_rs::subscribe(Duration::from_millis(500));
    let start = std::time::Instant::now();

    while start.elapsed() < Duration::from_secs(15) {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(info) => {
                let elapsed = start.elapsed().as_secs_f32();
                let status = if info.playing { "▶️ " } else { "⏸️ " };
                println!(
                    "   [{:5.1}s] {} {} — {} ({})",
                    elapsed,
                    status,
                    truncate(&info.title, 25),
                    info.artist.as_deref().unwrap_or("?"),
                    info.bundle_identifier
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No change detected, that's fine
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                println!("   Subscription channel closed.");
                break;
            }
        }
    }

    println!("\n✅ Test complete!");
}
