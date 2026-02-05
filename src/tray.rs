//! System tray implementation
//!
//! This module provides the system tray icon and menu for Brain.fm presence.
//! Uses the `tray-icon` crate for native cross-platform tray support.

use anyhow::{Context, Result};
use std::sync::mpsc::{self, Receiver, Sender};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

/// Menu item identifiers
const MENU_ID_STATUS: &str = "status";
const MENU_ID_QUIT: &str = "quit";

/// Events from the tray icon
#[derive(Debug, Clone)]
pub enum TrayEvent {
    /// User clicked Quit
    Quit,
}

/// System tray manager
pub struct TrayManager {
    _tray_icon: TrayIcon,
    status_item: MenuItem,
    event_receiver: Receiver<TrayEvent>,
}

impl TrayManager {
    /// Create a new tray manager with the given icon
    pub fn new() -> Result<Self> {
        // Create the tray icon from embedded bytes
        let icon = Self::load_icon()?;
        
        // Create event channel
        let (event_sender, event_receiver) = mpsc::channel();
        
        // Create menu items
        let status_item = MenuItem::with_id(MENU_ID_STATUS, "Brain.fm Presence", false, None);
        let quit_item = MenuItem::with_id(MENU_ID_QUIT, "Quit", true, None);
        
        // Build menu
        let menu = Menu::new();
        menu.append(&status_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;
        
        // Set up menu event handling
        let sender = event_sender.clone();
        std::thread::spawn(move || {
            Self::handle_menu_events(sender);
        });
        
        // Create tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("Brain.fm Presence")
            .build()
            .context("Failed to create tray icon")?;
        
        Ok(Self {
            _tray_icon: tray_icon,
            status_item,
            event_receiver,
        })
    }
    
    /// Load the tray icon
    fn load_icon() -> Result<Icon> {
        // Try to load from assets first
        let icon_bytes = include_bytes!("../assets/tray_icon.png");
        
        let image = image::load_from_memory(icon_bytes)
            .context("Failed to load tray icon image")?
            .into_rgba8();
        
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        
        Icon::from_rgba(rgba, width, height)
            .context("Failed to create icon from RGBA data")
    }
    
    /// Handle menu events in a separate thread
    fn handle_menu_events(sender: Sender<TrayEvent>) {
        let receiver = MenuEvent::receiver();
        loop {
            if let Ok(event) = receiver.recv() {
                match event.id.0.as_str() {
                    MENU_ID_QUIT => {
                        let _ = sender.send(TrayEvent::Quit);
                    }
                    _ => {}
                }
            }
        }
    }
    
    /// Update the status text shown in the tray menu
    pub fn update_status(&self, status: &str) {
        self.status_item.set_text(status);
    }
    
    /// Check for pending tray events (non-blocking)
    pub fn try_recv_event(&self) -> Option<TrayEvent> {
        self.event_receiver.try_recv().ok()
    }
    
    /// Wait for the next tray event (blocking)
    pub fn recv_event(&self) -> Option<TrayEvent> {
        self.event_receiver.recv().ok()
    }
}
