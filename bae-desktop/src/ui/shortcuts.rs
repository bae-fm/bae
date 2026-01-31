//! App-level keyboard shortcuts
//!
//! Maps Cmd+N (macOS) / Ctrl+N (Windows/Linux) to navigation actions.
//! Also provides a mechanism for native menus to request navigation.

use crate::ui::Route;
#[cfg(target_os = "macos")]
use bae_core::playback::RepeatMode;
use dioxus::prelude::*;
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// Navigation actions that can be triggered by shortcuts or menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Back,
    Forward,
    GoTo(NavTarget),
}

/// Navigation targets for direct routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTarget {
    Library,
    Import,
}

impl NavTarget {
    pub fn to_route(self) -> Route {
        match self {
            NavTarget::Library => Route::Library {},
            NavTarget::Import => Route::ImportWorkflowManager {},
        }
    }
}

static NAV_SENDER: OnceLock<broadcast::Sender<NavAction>> = OnceLock::new();

#[cfg(target_os = "macos")]
static PLAYBACK_SENDER: OnceLock<broadcast::Sender<PlaybackAction>> = OnceLock::new();

/// Initialize the navigation channel. Call once at startup.
pub fn init_nav_channel() {
    let (tx, _rx) = broadcast::channel(16);
    NAV_SENDER.set(tx).expect("nav channel already initialized");
}

/// Initialize the playback action channel. Call once at startup.
#[cfg(target_os = "macos")]
pub fn init_playback_channel() {
    let (tx, _rx) = broadcast::channel(16);
    PLAYBACK_SENDER
        .set(tx)
        .expect("playback channel already initialized");
}

/// Subscribe to navigation actions. Can be called multiple times (survives remounts).
pub fn subscribe_nav() -> broadcast::Receiver<NavAction> {
    NAV_SENDER
        .get()
        .expect("nav channel not initialized")
        .subscribe()
}

/// Subscribe to playback actions.
#[cfg(target_os = "macos")]
pub fn subscribe_playback_actions() -> broadcast::Receiver<PlaybackAction> {
    PLAYBACK_SENDER
        .get()
        .expect("playback channel not initialized")
        .subscribe()
}

/// Request a navigation action (called from native menu handlers).
/// On macOS, dispatches to main thread via GCD.
#[cfg(target_os = "macos")]
pub fn request_nav(action: NavAction) {
    dispatch::Queue::main().exec_async(move || {
        if let Some(tx) = NAV_SENDER.get() {
            let _ = tx.send(action);
        }
    });
}

/// Request a playback action (called from native menu handlers).
/// On macOS, dispatches to main thread via GCD.
#[cfg(target_os = "macos")]
pub fn request_playback_action(action: PlaybackAction) {
    dispatch::Queue::main().exec_async(move || {
        if let Some(tx) = PLAYBACK_SENDER.get() {
            let _ = tx.send(action);
        }
    });
}

/// Playback actions that can be triggered by native menus.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAction {
    SetRepeatMode(RepeatMode),
    TogglePlayPause,
    Next,
    Previous,
}

/// Check if the platform modifier key is pressed (Cmd on macOS, Ctrl elsewhere).
fn has_platform_modifier(evt: &KeyboardEvent) -> bool {
    let mods = evt.modifiers();
    #[cfg(target_os = "macos")]
    {
        mods.meta() && !mods.ctrl() && !mods.alt() && !mods.shift()
    }
    #[cfg(not(target_os = "macos"))]
    {
        mods.ctrl() && !mods.meta() && !mods.alt() && !mods.shift()
    }
}

/// Try to handle a keyboard event as an app shortcut.
/// Returns `Some(NavAction)` if the event matches a shortcut, `None` otherwise.
pub fn handle_shortcut(evt: &KeyboardEvent) -> Option<NavAction> {
    if !has_platform_modifier(evt) {
        return None;
    }

    // On macOS, Cmd+1/2 and Cmd+[/] are handled by the native menu
    // and won't reach the webview. Only non-menu shortcuts need to go here.
    #[cfg(not(target_os = "macos"))]
    match evt.key() {
        Key::Character(c) if c == "1" => return Some(NavAction::GoTo(NavTarget::Library)),
        Key::Character(c) if c == "2" => return Some(NavAction::GoTo(NavTarget::Import)),
        Key::Character(c) if c == "[" => return Some(NavAction::Back),
        Key::Character(c) if c == "]" => return Some(NavAction::Forward),
        _ => {}
    }

    None
}

fn execute_nav_action(action: NavAction) {
    match action {
        NavAction::Back => navigator().go_back(),
        NavAction::Forward => navigator().go_forward(),
        NavAction::GoTo(target) => {
            let _ = navigator().push(target.to_route());
        }
    }
}

#[component]
pub fn ShortcutsHandler(children: Element) -> Element {
    // Listen for menu-triggered navigation (subscribes fresh on each mount)
    use_hook(|| {
        let mut rx = subscribe_nav();
        spawn(async move {
            while let Ok(action) = rx.recv().await {
                execute_nav_action(action);
            }
        });
    });

    let onkeydown = move |evt: KeyboardEvent| {
        if let Some(action) = handle_shortcut(&evt) {
            evt.prevent_default();
            execute_nav_action(action);
        }
    };

    rsx! {
        div { class: "contents", onkeydown, {children} }
    }
}
