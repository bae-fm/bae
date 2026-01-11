//! Sparkle auto-update integration for macOS
//!
//! This module provides FFI bindings to Sparkle.framework for automatic updates.
//! Sparkle checks for updates on launch and provides a manual "Check for Updates" option.

#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use tracing::error;
use tracing::info;

/// Initialize the Sparkle updater and start background update checks.
/// Call this early in app startup (after UI is ready to handle dialogs).
pub fn start() {
    #[cfg(target_os = "macos")]
    {
        info!("Initializing Sparkle updater");

        unsafe {
            let updater_class = match Class::get("SPUStandardUpdaterController") {
                Some(class) => class,
                None => {
                    error!("Sparkle framework not loaded - SPUStandardUpdaterController class not found");
                    return;
                }
            };

            // Get or create the shared updater controller
            let controller: *mut Object = msg_send![updater_class, alloc];
            let controller: *mut Object = msg_send![controller, initWithStartingUpdater:true updaterDelegate:std::ptr::null::<Object>() userDriverDelegate:std::ptr::null::<Object>()];

            if controller.is_null() {
                error!("Failed to initialize Sparkle updater controller");
                return;
            }

            // Store in static so it persists
            UPDATER_CONTROLLER.store(controller as usize, std::sync::atomic::Ordering::SeqCst);

            info!("Sparkle updater initialized - automatic update checks enabled");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        info!("Auto-update not available on this platform");
    }
}

/// Manually check for updates (triggered by user action).
/// Shows the update dialog if an update is available.
pub fn check_for_updates() {
    #[cfg(target_os = "macos")]
    {
        info!("Checking for updates...");

        unsafe {
            let controller_ptr =
                UPDATER_CONTROLLER.load(std::sync::atomic::Ordering::SeqCst) as *mut Object;

            if controller_ptr.is_null() {
                error!("Sparkle updater not initialized");
                return;
            }

            // Get the updater from the controller
            let updater: *mut Object = msg_send![controller_ptr, updater];
            if updater.is_null() {
                error!("Failed to get Sparkle updater instance");
                return;
            }

            // Trigger manual update check (shows UI)
            let _: () = msg_send![updater, checkForUpdates];
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        info!("Auto-update not available on this platform");
    }
}

#[cfg(target_os = "macos")]
static UPDATER_CONTROLLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
