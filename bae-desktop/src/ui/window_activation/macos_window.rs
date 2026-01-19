#[cfg(target_os = "macos")]
use cocoa::appkit::{
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSMenuItem, NSWindow, NSWindowStyleMask,
    NSWindowTitleVisibility,
};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil, selector, NO, YES};
#[cfg(target_os = "macos")]
use cocoa::foundation::{NSAutoreleasePool, NSString};
#[cfg(target_os = "macos")]
use dispatch::Queue;
#[cfg(target_os = "macos")]
use objc::declare::ClassDecl;
#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
use tracing::info;

#[cfg(target_os = "macos")]
static MENU_HANDLER_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();
#[cfg(target_os = "macos")]
static MENU_DELEGATE_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();
#[cfg(target_os = "macos")]
static UPDATE_MENU_ITEM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "macos")]
pub fn setup_macos_window_activation() {
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        app.setActivationPolicy_(
            NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
        );
        app.activateIgnoringOtherApps_(cocoa::base::YES);
        info!("macOS window activation configured");
    }
}
/// Register a custom Objective-C class to handle menu actions
#[cfg(target_os = "macos")]
unsafe fn register_menu_handler_class() {
    MENU_HANDLER_CLASS_REGISTERED.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("BaeMenuHandler", superclass).unwrap();

        extern "C" fn check_for_updates(_this: &Object, _cmd: Sel, _sender: id) {
            crate::updater::check_for_updates();
        }

        decl.add_method(
            sel!(checkForUpdates:),
            check_for_updates as extern "C" fn(&Object, Sel, id),
        );

        decl.register();
    });
}

/// Get or create the shared menu handler instance
#[cfg(target_os = "macos")]
unsafe fn get_menu_handler() -> id {
    register_menu_handler_class();

    static HANDLER: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let handler_ptr = HANDLER.get_or_init(|| {
        let class = Class::get("BaeMenuHandler").unwrap();
        let handler: id = msg_send![class, alloc];
        let handler: id = msg_send![handler, init];
        handler as usize
    });
    *handler_ptr as id
}

/// Register a menu delegate class that updates menu item titles before display
#[cfg(target_os = "macos")]
unsafe fn register_menu_delegate_class() {
    MENU_DELEGATE_CLASS_REGISTERED.call_once(|| {
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("BaeMenuDelegate", superclass).unwrap();

        // Called when menu is about to open - update the update menu item title and state
        extern "C" fn menu_will_open(_this: &Object, _cmd: Sel, _menu: id) {
            unsafe {
                let item_ptr =
                    UPDATE_MENU_ITEM.load(std::sync::atomic::Ordering::SeqCst) as *mut Object;
                if !item_ptr.is_null() {
                    let state = crate::updater::update_state();
                    let (title, enabled) = match state {
                        crate::updater::UpdateState::Downloading => (
                            NSString::alloc(nil).init_str("Downloading Update..."),
                            false,
                        ),
                        crate::updater::UpdateState::Ready => {
                            (NSString::alloc(nil).init_str("Restart to Update..."), true)
                        }
                        crate::updater::UpdateState::Idle => {
                            (NSString::alloc(nil).init_str("Check for Updates..."), true)
                        }
                    };
                    let _: () = msg_send![item_ptr, setTitle: title];
                    let _: () = msg_send![item_ptr, setEnabled: enabled];
                }
            }
        }

        decl.add_method(
            sel!(menuWillOpen:),
            menu_will_open as extern "C" fn(&Object, Sel, id),
        );

        decl.register();
    });
}

/// Get or create the shared menu delegate instance
#[cfg(target_os = "macos")]
unsafe fn get_menu_delegate() -> id {
    register_menu_delegate_class();

    static DELEGATE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let delegate_ptr = DELEGATE.get_or_init(|| {
        let class = Class::get("BaeMenuDelegate").unwrap();
        let delegate: id = msg_send![class, alloc];
        let delegate: id = msg_send![delegate, init];
        delegate as usize
    });
    *delegate_ptr as id
}

/// Set up the application menu with custom items including "Check for Updates..."
/// Dispatches to main thread since Cocoa UI operations require it.
#[cfg(target_os = "macos")]
pub fn setup_app_menu() {
    Queue::main().exec_async(|| unsafe {
        let app = NSApplication::sharedApplication(nil);
        setup_app_menu_inner(app);
    });
}

#[cfg(target_os = "macos")]
unsafe fn setup_app_menu_inner(app: id) {
    let _pool = NSAutoreleasePool::new(nil);
    let main_menu = NSMenu::new(nil);
    main_menu.autorelease();
    let app_menu = NSMenu::new(nil);
    app_menu.autorelease();

    // Set menu delegate to update titles dynamically
    let menu_delegate = get_menu_delegate();
    let _: () = msg_send![app_menu, setDelegate: menu_delegate];

    // About bae
    let about_title = NSString::alloc(nil).init_str("About bae");
    let about_key = NSString::alloc(nil).init_str("");
    let about_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        about_title,
        selector("orderFrontStandardAboutPanel:"),
        about_key,
    );
    about_item.autorelease();
    app_menu.addItem_(about_item);

    // Check for Updates... (title updated dynamically by menu delegate)
    let update_title = NSString::alloc(nil).init_str("Check for Updates...");
    let update_key = NSString::alloc(nil).init_str("");
    let update_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        update_title,
        selector("checkForUpdates:"),
        update_key,
    );
    // Don't autorelease - we need to keep a reference for dynamic title updates
    let _: () = msg_send![update_item, retain];
    UPDATE_MENU_ITEM.store(update_item as usize, std::sync::atomic::Ordering::SeqCst);
    let menu_handler = get_menu_handler();
    let _: () = msg_send![update_item, setTarget: menu_handler];
    app_menu.addItem_(update_item);

    let separator1 = NSMenuItem::separatorItem(nil);
    app_menu.addItem_(separator1);

    let close_title = NSString::alloc(nil).init_str("Close Window");
    let close_key = NSString::alloc(nil).init_str("w");
    let close_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        close_title,
        selector("performClose:"),
        close_key,
    );
    close_item.autorelease();
    app_menu.addItem_(close_item);

    let minimize_title = NSString::alloc(nil).init_str("Minimize");
    let minimize_key = NSString::alloc(nil).init_str("m");
    let minimize_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        minimize_title,
        selector("performMiniaturize:"),
        minimize_key,
    );
    minimize_item.autorelease();
    app_menu.addItem_(minimize_item);

    let separator2 = NSMenuItem::separatorItem(nil);
    app_menu.addItem_(separator2);

    let quit_title = NSString::alloc(nil).init_str("Quit bae");
    let quit_key = NSString::alloc(nil).init_str("q");
    let quit_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        quit_title,
        selector("terminate:"),
        quit_key,
    );
    quit_item.autorelease();
    app_menu.addItem_(quit_item);

    let app_menu_item = NSMenuItem::new(nil);
    app_menu_item.autorelease();
    app_menu_item.setSubmenu_(app_menu);
    main_menu.addItem_(app_menu_item);
    app.setMainMenu_(main_menu);
}
/// Configure the window with transparent titlebar and native traffic lights.
/// This must be called after the window is created.
/// Dispatches to main thread since Cocoa UI operations require it.
#[cfg(target_os = "macos")]
pub fn setup_transparent_titlebar() {
    Queue::main().exec_async(|| {
        setup_transparent_titlebar_inner();
    });
}

#[cfg(target_os = "macos")]
fn setup_transparent_titlebar_inner() {
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        let windows: id = msg_send![app, windows];
        let count: usize = msg_send![windows, count];

        if count == 0 {
            info!("Warning: No window found for transparent titlebar setup");
            return;
        }

        let window: id = msg_send![windows, objectAtIndex: 0usize];

        window.setTitlebarAppearsTransparent_(YES);
        window.setTitleVisibility_(NSWindowTitleVisibility::NSWindowTitleHidden);

        let current_style_mask: NSWindowStyleMask = window.styleMask();
        let new_style_mask =
            current_style_mask | NSWindowStyleMask::NSFullSizeContentViewWindowMask;
        window.setStyleMask_(new_style_mask);

        let toolbar: id = msg_send![class!(NSToolbar), alloc];
        let toolbar: id = msg_send![
            toolbar, initWithIdentifier : NSString::alloc(nil).init_str("MainToolbar")
        ];
        let _: () = msg_send![toolbar, setShowsBaselineSeparator : NO];
        let _: () = msg_send![window, setToolbar : toolbar];

        // Zoom window to fill the screen
        let screen: id = msg_send![window, screen];
        if screen != nil {
            let frame: cocoa::foundation::NSRect = msg_send![screen, visibleFrame];
            let _: () = msg_send![window, setFrame: frame display: YES];
        }

        info!("macOS transparent titlebar configured");
    }
}
