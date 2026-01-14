//! Sparkle auto-update integration for macOS
//!
//! This module provides FFI bindings to Sparkle.framework for automatic updates.
//! Sparkle checks for updates on launch and provides a manual "Check for Updates" option.

#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use cocoa::foundation::NSString;
#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
pub fn update_state() -> UpdateState {
    if UPDATE_READY.load(std::sync::atomic::Ordering::SeqCst) {
        UpdateState::Ready
    } else if UPDATE_DOWNLOADING.load(std::sync::atomic::Ordering::SeqCst) {
        UpdateState::Downloading
    } else {
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

        // Called when Sparkle starts checking for updates
        extern "C" fn updater_will_check_for_update(_this: &Object, _cmd: Sel, _updater: id) {
            info!("Starting automatic update check...");
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
            sel!(updaterWillCheckForUpdate:),
            updater_will_check_for_update as extern "C" fn(&Object, Sel, id),
        );
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

/// Load Sparkle.framework from the app bundle
#[cfg(target_os = "macos")]
unsafe fn load_sparkle_framework() -> bool {
    // Get the main bundle
    let bundle_class = class!(NSBundle);
    let main_bundle: id = msg_send![bundle_class, mainBundle];
    if main_bundle.is_null() {
        error!("Failed to get main bundle");
        return false;
    }

    // Get the Frameworks path
    let frameworks_path: id = msg_send![main_bundle, privateFrameworksPath];
    if frameworks_path.is_null() {
        error!("Failed to get frameworks path");
        return false;
    }

    // Build path to Sparkle.framework
    let sparkle_path: id = msg_send![frameworks_path, stringByAppendingPathComponent: NSString::alloc(nil).init_str("Sparkle.framework")];

    // Load the bundle
    let sparkle_bundle: id = msg_send![bundle_class, bundleWithPath: sparkle_path];
    if sparkle_bundle.is_null() {
        error!("Sparkle.framework not found in app bundle");
        return false;
    }

    let loaded: bool = msg_send![sparkle_bundle, load];
    if !loaded {
        error!("Failed to load Sparkle.framework");
        return false;
    }

    info!("Sparkle.framework loaded successfully");
    true
}

/// Initialize the Sparkle updater and start background update checks.
/// Call this early in app startup (after UI is ready to handle dialogs).
pub fn start() {
    #[cfg(target_os = "macos")]
    {
        info!("Initializing Sparkle updater");

        unsafe {
            // First, load the framework
            if !load_sparkle_framework() {
                return;
            }

            let updater_class = match Class::get("SPUStandardUpdaterController") {
                Some(class) => class,
                None => {
                    error!(
                        "Sparkle framework loaded but SPUStandardUpdaterController class not found"
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

            // Get the updater and trigger an immediate background check
            let updater: *mut Object = msg_send![controller, updater];
            if !updater.is_null() {
                let _: () = msg_send![updater, checkForUpdatesInBackground];
                info!("Sparkle updater initialized - triggered background update check");
            } else {
                info!("Sparkle updater initialized - automatic update checks enabled");
            }
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
