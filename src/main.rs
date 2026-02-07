//! Brain.fm Discord Rich Presence
//! 
//! This is a proof-of-concept that reads the current Brain.fm state
//! and displays it for potential Discord Rich Presence integration.

use anyhow::Result;
use brainfm_presence::{BrainFmReader, BrainFmState};
use brainfm_presence::util::truncate;

fn main() -> Result<()> {
    println!("🧠 Brain.fm Presence Reader - PoC");
    println!("==================================\n");
    
    // Create reader
    let mut reader = match BrainFmReader::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("❌ Error: {e}");
            eprintln!("\nMake sure Brain.fm is installed and has been run at least once.");
            return Err(e);
        }
    };
    
    // Check if Brain.fm is running
    if reader.is_running() {
        println!("✅ Brain.fm is running!\n");
    } else {
        println!("⚠️  Brain.fm is not currently running.");
        println!("   Start Brain.fm and run this program again.\n");
        
        // Still try to read cached state from LevelDB
        println!("📁 Reading cached state from local storage...\n");
    }
    
    // Read current state
    println!("📊 Reading Brain.fm state...\n");
    
    match reader.read_state() {
        Ok(state) => {
            print_state(&state);
            
            println!("\n📝 For Discord Rich Presence:");
            println!("   State: {}", state.to_presence_string());
            if let Some(details) = state.to_details_string() {
                println!("   Details: {details}");
            }
        }
        Err(e) => {
            eprintln!("❌ Error reading state: {e}");
        }
    }
    
    // Also run individual readers for debugging
    println!("\n\n🔍 Debug: Individual Reader Results");
    println!("=====================================\n");
    
    // LevelDB reader
    println!("📂 LevelDB Reader:");
    match brainfm_presence::leveldb_reader::read_state(
        &dirs::home_dir()
            .unwrap()
            .join("Library/Application Support/Brain.fm"),
    ) {
        Ok(state) => print_state_compact(&state, "   "),
        Err(e) => println!("   ❌ Error: {e}"),
    }
    
    // Cache reader (standalone, without API cache enrichment)
    println!("\n💾 Cache Reader (standalone):");
    match brainfm_presence::cache_reader::read_state(
        &dirs::home_dir()
            .unwrap()
            .join("Library/Application Support/Brain.fm"),
        None,
    ) {
        Ok(state) => print_state_compact(&state, "   "),
        Err(e) => println!("   ❌ Error: {e}"),
    }

    // Direct API client
    println!("\n🔑 Direct API Client:");
    let app_path = dirs::home_dir()
        .unwrap()
        .join("Library/Application Support/Brain.fm");
    match brainfm_presence::api_client::fetch_recent_tracks(&app_path) {
        Ok(Some(data)) => {
            println!("   ✅ Fetched {} tracks from live API", data.len());
        }
        Ok(None) => {
            println!("   ⏭️  Skipped (token expired or unavailable)");
        }
        Err(e) => println!("   ❌ Error: {e}"),
    }

    // API cache reader (fallback)
    println!("\n🌐 API Cache Reader (fallback):");
    match brainfm_presence::api_cache_reader::read_api_cache(&app_path) {
        Ok(cache) => {
            if cache.is_empty() {
                println!("   (no cached API data found)");
            } else {
                println!("   ✅ Found {} tracks in disk cache", cache.len());
            }
        }
        Err(e) => println!("   ❌ Error: {e}"),
    }

    // MediaRemote reader (macOS Now Playing)
    println!("\n🎵 MediaRemote Reader (macOS Now Playing):");
    match brainfm_presence::media_remote_reader::read_state() {
        Some(mr) => {
            println!("   ✅ Brain.fm detected via MediaRemote");
            println!("   Playing: {} | Track: {} | Elapsed: {:.0}s / {:.0}s",
                if mr.is_playing { "Yes" } else { "No" },
                mr.track_name.as_deref().unwrap_or("(none)"),
                mr.elapsed_secs.unwrap_or(0.0),
                mr.duration_secs.unwrap_or(0.0),
            );
        }
        None => {
            println!("   (Brain.fm not detected as Now Playing app)");
        }
    }
    
    Ok(())
}

fn print_state(state: &BrainFmState) {
    println!("┌─────────────────────────────────────┐");
    println!("│ 🧠 Brain.fm Current State           │");
    println!("├─────────────────────────────────────┤");
    
    if let Some(ref mode) = state.mode {
        println!("│ Mode:          {mode:20} │");
    } else {
        println!("│ Mode:          {:20} │", "(unknown)");
    }
    
    println!("│ Playing:       {:20} │", if state.is_playing { "Yes ▶️" } else { "No ⏸️" });
    
    if let Some(ref session_state) = state.session_state {
        println!("│ Session:       {session_state:20} │");
    }
    
    if let Some(ref time) = state.session_time {
        println!("│ Time:          {time:20} │");
    }
    
    if let Some(ref track) = state.track_name {
        println!("│ Track:         {:20} │", truncate(track, 20));
    }

    if let Some(ref effect) = state.neural_effect {
        println!("│ Neural Effect: {:20} │", truncate(effect, 20));
    }

    if let Some(ref genre) = state.genre {
        println!("│ Genre:         {genre:20} │");
    }

    if let Some(ref activity) = state.activity {
        println!("│ Activity:      {activity:20} │");
    }

    if let Some(ref image_url) = state.image_url {
        println!("│ Image:         {:20} │", truncate(image_url, 20));
    }
    
    if state.infinite_play {
        println!("│ Infinite Play: {:20} │", "Enabled ∞");
    }
    
    if state.adhd_mode {
        println!("│ ADHD Mode:     {:20} │", "Enabled 🧠");
    }
    
    println!("└─────────────────────────────────────┘");
}

fn print_state_compact(state: &BrainFmState, prefix: &str) {
    let mut fields = Vec::new();
    
    if let Some(ref mode) = state.mode {
        fields.push(format!("Mode: {mode}"));
    }
    if state.is_playing {
        fields.push("Playing: Yes".to_string());
    }
    if let Some(ref time) = state.session_time {
        fields.push(format!("Time: {time}"));
    }
    if state.adhd_mode {
        fields.push("ADHD: Yes".to_string());
    }
    
    if fields.is_empty() {
        println!("{prefix}(no data)");
    } else {
        println!("{}{}", prefix, fields.join(" | "));
    }
}
