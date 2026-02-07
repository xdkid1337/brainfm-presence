//! Brain.fm Discord Rich Presence - System Tray Application
//!
//! This binary provides Discord Rich Presence integration for Brain.fm.
//! It runs as a system tray application without a visible window.
//!
//! Architecture:
//! - Main thread: runs winit event loop for proper macOS menu handling
//! - Background thread: reads Brain.fm state and updates Discord

use brainfm_presence::{BrainFmReader, BrainFmState};
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use log::{debug, error, info, warn};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

/// Discord Application ID
const DISCORD_APP_ID: &str = "1468727702675521547";

/// Update interval in seconds
const UPDATE_INTERVAL_SECS: u64 = 5;

/// Menu item IDs
const MENU_ID_STATUS: &str = "status";
const MENU_ID_QUIT: &str = "quit";

/// Events sent from background thread to main thread
#[derive(Debug, Clone)]
enum UserEvent {
    /// Status update from background thread
    StatusUpdate(String),
    /// Menu event from tray
    MenuEvent(tray_icon::menu::MenuEvent),
}

/// Application state
struct App {
    status_item: MenuItem,
    _tray_icon: tray_icon::TrayIcon,
    shutdown_tx: mpsc::Sender<()>,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Not used for tray-only app
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {
        // No windows in tray-only app
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::StatusUpdate(status) => {
                self.status_item.set_text(&status);
            }
            UserEvent::MenuEvent(menu_event) => {
                if menu_event.id.0 == MENU_ID_QUIT {
                    info!("Quit requested, shutting down...");
                    // Signal background thread to stop
                    let _ = self.shutdown_tx.send(());
                    event_loop.exit();
                }
            }
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    info!("🧠 Brain.fm Discord Rich Presence starting...");

    // Create event loop with custom user events
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    // Set control flow to wait (efficient, no busy loop)
    event_loop.set_control_flow(ControlFlow::Wait);

    // Create event loop proxy for sending events from background thread
    let proxy = event_loop.create_proxy();

    // Set up menu event handler to forward to event loop
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::MenuEvent(event));
    }));

    // Create tray icon and menu
    let (tray_icon, status_item) = create_tray_icon();

    info!("✅ System tray initialized");

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    // Spawn background thread for Brain.fm reading and Discord updates
    thread::spawn(move || {
        run_background_worker(proxy, shutdown_rx);
    });

    // Create app handler
    let mut app = App {
        status_item,
        _tray_icon: tray_icon,
        shutdown_tx,
    };

    // Run the event loop (this blocks and handles all events properly)
    info!("🔄 Running event loop...");
    let _ = event_loop.run_app(&mut app);
}

/// Create the tray icon and menu
fn create_tray_icon() -> (tray_icon::TrayIcon, MenuItem) {
    // Load icon
    let icon = load_icon();

    // Create menu items
    let status_item = MenuItem::with_id(MENU_ID_STATUS, "Brain.fm Presence", false, None);
    let quit_item = MenuItem::with_id(MENU_ID_QUIT, "Quit", true, None);

    // Build menu
    let menu = Menu::new();
    menu.append(&status_item).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&quit_item).unwrap();

    // Create tray icon
    let tray_icon = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_tooltip("Brain.fm Presence")
        .build()
        .expect("Failed to create tray icon");

    (tray_icon, status_item)
}

/// Load the tray icon
fn load_icon() -> Icon {
    let icon_bytes = include_bytes!("../../assets/tray_icon.png");

    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load tray icon image")
        .into_rgba8();

    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    Icon::from_rgba(rgba, width, height).expect("Failed to create icon from RGBA data")
}

/// Background worker that reads Brain.fm state and updates Discord
fn run_background_worker(proxy: winit::event_loop::EventLoopProxy<UserEvent>, shutdown_rx: mpsc::Receiver<()>) {
    // Create Brain.fm reader
    let mut reader = match BrainFmReader::new() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create Brain.fm reader: {}", e);
            error!("Make sure Brain.fm is installed and has been run at least once.");
            return;
        }
    };

    // Try to connect to Discord
    info!("🔗 Connecting to Discord...");
    let mut client = create_discord_client();
    
    if client.is_some() {
        info!("✅ Connected to Discord!");
    } else {
        warn!("Discord not available, will retry in background");
    }

    let mut last_state: Option<BrainFmState> = None;
    let mut track_start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut last_track: Option<String> = None;
    let mut discord_retry_count = 0;

    loop {
        // Check for shutdown signal
        if shutdown_rx.try_recv().is_ok() {
            info!("Background worker shutting down...");
            if let Some(ref mut c) = client {
                let _ = c.clear_activity();
                let _ = c.close();
            }
            break;
        }

        // Try to reconnect to Discord if not connected
        if client.is_none() && discord_retry_count % 4 == 0 {
            if let Some(c) = create_discord_client() {
                info!("Connected to Discord!");
                client = Some(c);
            }
        }
        discord_retry_count += 1;

        // Read current Brain.fm state
        match reader.read_state() {
            Ok(state) => {
                // Check if track changed - reset timer
                let current_track = state.track_name.clone();
                if current_track != last_track {
                    track_start = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    last_track = current_track;
                }

                // Send status update to main thread
                let status_text = format_status(&state);
                let _ = proxy.send_event(UserEvent::StatusUpdate(status_text.clone()));

                // Update Discord if connected
                if let Some(ref mut c) = client {
                    let should_update = match &last_state {
                        None => true,
                        Some(last) => state_changed(last, &state),
                    };

                    if should_update {
                        if let Err(e) = update_discord_presence(c, &state, track_start) {
                            warn!("Discord update error: {}", e);
                            // Connection might be lost, try to reconnect
                            client = None;
                        } else {
                            debug!("Updated presence: {}", status_text);
                        }
                        last_state = Some(state);
                    }
                }
            }
            Err(e) => {
                debug!("Error reading state: {}", e);
                let _ = proxy.send_event(UserEvent::StatusUpdate("Brain.fm not running".to_string()));
            }
        }

        // Sleep for update interval
        thread::sleep(Duration::from_secs(UPDATE_INTERVAL_SECS));
    }
}

/// Create and connect Discord client
fn create_discord_client() -> Option<DiscordIpcClient> {
    let mut client = DiscordIpcClient::new(DISCORD_APP_ID);

    // Try to connect with timeout
    for _ in 0..3 {
        if client.connect().is_ok() {
            return Some(client);
        }
        thread::sleep(Duration::from_millis(500));
    }

    None
}

/// Format status text for tray menu
fn format_status(state: &BrainFmState) -> String {
    if !state.is_playing {
        return "Not playing".to_string();
    }

    let mut parts = Vec::new();

    if let Some(ref mode) = state.mode {
        parts.push(mode.clone());
    }

    if let Some(ref track) = state.track_name {
        parts.push(track.clone());
    }

    if let Some(ref genre) = state.genre {
        parts.push(genre.clone());
    }

    if parts.is_empty() {
        "Playing...".to_string()
    } else {
        parts.join(" - ")
    }
}

/// Check if state has changed enough to warrant an update
fn state_changed(old: &BrainFmState, new: &BrainFmState) -> bool {
    old.is_playing != new.is_playing
        || old.mode != new.mode
        || old.track_name != new.track_name
        || old.neural_effect != new.neural_effect
        || old.genre != new.genre
        || old.activity != new.activity
}

/// Update Discord presence with current state
fn update_discord_presence(
    client: &mut DiscordIpcClient,
    state: &BrainFmState,
    session_start: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    if !state.is_playing {
        client.clear_activity()?;
        return Ok(());
    }

    // Build strings: details = track name, state = mode (or activity)
    let state_text = state.mode.clone().unwrap_or_else(|| "Focus".to_string());
    let details = state.track_name.clone().unwrap_or_else(|| "Brain.fm".to_string());

    // Large image: prefer track-specific image from API cache, fall back to mode image from CDN
    let large_image_owned;
    let large_image = if let Some(ref url) = state.image_url {
        large_image_owned = url.clone();
        large_image_owned.as_str()
    } else {
        match state.mode.as_deref() {
            Some("Sleep") | Some("Deep Sleep") | Some("Light Sleep") => {
                "https://cdn.brain.fm/images/sleep/sleep_mental_state_bg_small_aura.webp"
            }
            Some("Relax") | Some("Recharge") | Some("Chill") => {
                "https://cdn.brain.fm/images/relax/relax_mental_state_bg_small_aura.webp"
            }
            Some("Meditate") | Some("Unguided") | Some("Guided") => {
                "https://cdn.brain.fm/images/meditate/meditate_mental_state_bg_small_aura.webp"
            }
            _ => "https://cdn.brain.fm/images/focus/focus_mental_state_bg_small_aura.webp",
        }
    };
    let large_text = state
        .neural_effect
        .clone()
        .unwrap_or_else(|| "Neural Effect Level".to_string());

    // Small image = genre from Brain.fm CDN
    let small_image = match state.genre.as_deref() {
        Some("LoFi") | Some("Lofi") | Some("lofi") => "https://cdn.brain.fm/icons/lofi.png",
        Some("Piano") | Some("piano") => "https://cdn.brain.fm/icons/piano.png",
        Some("Electronic") | Some("electronic") => "https://cdn.brain.fm/icons/electronic.png",
        Some("Grooves") | Some("grooves") => "https://cdn.brain.fm/icons/grooves.png",
        Some("Atmospheric") | Some("atmospheric") => "https://cdn.brain.fm/icons/atmospheric.png",
        Some("Cinematic") | Some("cinematic") => "https://cdn.brain.fm/icons/cinematic.png",
        Some("Classical") | Some("classical") => "https://cdn.brain.fm/icons/classical.png",
        Some("Acoustic") | Some("acoustic") => "https://cdn.brain.fm/icons/acoustic.png",
        Some("Drone") | Some("drone") => "https://cdn.brain.fm/icons/drone.png",
        Some("Rain") | Some("rain") => "https://cdn.brain.fm/icons/rain.png",
        Some("Forest") | Some("forest") => "https://cdn.brain.fm/icons/forest.png",
        Some("Beach") | Some("beach") => "https://cdn.brain.fm/icons/beach.png",
        Some("Night") | Some("night") => "https://cdn.brain.fm/icons/night.png",
        _ => "https://cdn.brain.fm/icons/electronic.png",
    };
    let small_text = state.genre.clone().unwrap_or_else(|| "Brain.fm".to_string());

    // Build activity with ActivityType::Listening for "Listening to brain.fm"
    let timestamps = activity::Timestamps::new().start(session_start);

    let assets = activity::Assets::new()
        .large_image(large_image)
        .large_text(&large_text)
        .small_image(small_image)
        .small_text(&small_text);

    let activity_payload = activity::Activity::new()
        .activity_type(activity::ActivityType::Listening)
        .state(&state_text)
        .details(&details)
        .timestamps(timestamps)
        .assets(assets);

    client.set_activity(activity_payload)?;

    Ok(())
}

