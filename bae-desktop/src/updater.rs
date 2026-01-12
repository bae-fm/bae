//! Sparkle auto-update integration for macOS
//!
//! This module provides FFI bindings to Sparkle.framework for automatic updates.
//! Sparkle checks for updates on launch and provides a manual "Check for Updates" option.

#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use tracing::error;
use tracing::info;

#[cfg(target_os = "macos")]
static UPDATER_CONTROLLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static UPDATE_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "macos")]
static UPDATE_DOWNLOADING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "macos")]
static DELEGATE_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();

/// Update state for menu display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateState {
    /// No update activity
    Idle,
    /// Currently downloading an update
    Downloading,
    /// Update downloaded and ready to install
    Ready,
}

/// Returns the current update state
pub fn update_state() -> UpdateState {
    #[cfg(target_os = "macos")]
    {
        if UPDATE_READY.load(std::sync::atomic::Ordering::SeqCst) {
            UpdateState::Ready
        } else if UPDATE_DOWNLOADING.load(std::sync::atomic::Ordering::SeqCst) {
            UpdateState::Downloading
        } else {
            UpdateState::Idle
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        UpdateState::Idle
    }
}

#[cfg(target_os = "macos")]
unsafe fn register_delegate_class() {
    DELEGATE_CLASS_REGISTERED.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("BaeUpdaterDelegate", superclass).unwrap();

        // Called when update found and download is starting
        extern "C" fn updater_will_download_update(
            _this: &Object,
            _cmd: Sel,
            _updater: id,
            _item: id,
            _request: id,
        ) {
            info!("Downloading update...");
            UPDATE_DOWNLOADING.store(true, std::sync::atomic::Ordering::SeqCst);
            UPDATE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Called when an update is found and downloaded
        extern "C" fn updater_did_download_update(
            _this: &Object,
            _cmd: Sel,
            _updater: id,
            _item: id,
        ) {
            info!("Update downloaded and ready to install");
            UPDATE_DOWNLOADING.store(false, std::sync::atomic::Ordering::SeqCst);
            UPDATE_READY.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        // Called when update check didn't find an update
        extern "C" fn updater_did_not_find_update(_this: &Object, _cmd: Sel, _updater: id) {
            info!("No update available");
            UPDATE_DOWNLOADING.store(false, std::sync::atomic::Ordering::SeqCst);
            UPDATE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Called when update is cancelled or fails
        extern "C" fn updater_did_cancel_update(
            _this: &Object,
            _cmd: Sel,
            _updater: id,
            _error: id,
        ) {
            info!("Update cancelled or failed");
            UPDATE_DOWNLOADING.store(false, std::sync::atomic::Ordering::SeqCst);
            UPDATE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Called when download fails
        extern "C" fn updater_did_fail_download(
            _this: &Object,
            _cmd: Sel,
            _updater: id,
            _item: id,
            _error: id,
        ) {
            info!("Update download failed");
            UPDATE_DOWNLOADING.store(false, std::sync::atomic::Ordering::SeqCst);
            UPDATE_READY.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        decl.add_method(
            sel!(updater:willDownloadUpdate:withRequest:),
            updater_will_download_update as extern "C" fn(&Object, Sel, id, id, id),
        );
        decl.add_method(
            sel!(updater:didDownloadUpdate:),
            updater_did_download_update as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(updaterDidNotFindUpdate:),
            updater_did_not_find_update as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(updater:didCancelUpdateCheckWithError:),
            updater_did_cancel_update as extern "C" fn(&Object, Sel, id, id),
        );
        decl.add_method(
            sel!(updater:failedToDownloadUpdate:error:),
            updater_did_fail_download as extern "C" fn(&Object, Sel, id, id, id),
        );

        decl.register();
    });
}

#[cfg(target_os = "macos")]
unsafe fn create_delegate() -> id {
    register_delegate_class();
    let class = Class::get("BaeUpdaterDelegate").unwrap();
    let delegate: id = msg_send![class, alloc];
    let delegate: id = msg_send![delegate, init];
    delegate
}

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
                    error!(
                        "Sparkle framework not loaded - SPUStandardUpdaterController class not found"
                    );
                    return;
                }
            };

            let delegate = create_delegate();

            // Get or create the shared updater controller with our delegate
            let controller: *mut Object = msg_send![updater_class, alloc];
            let controller: *mut Object = msg_send![controller, initWithStartingUpdater:true updaterDelegate:delegate userDriverDelegate:nil];

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
