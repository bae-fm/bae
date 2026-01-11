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
use objc::{class, msg_send, sel, sel_impl};
use tracing::info;
#[cfg(target_os = "macos")]
pub fn setup_macos_window_activation() {
    unsafe {
        let app = NSApplication::sharedApplication(nil);
        app.setActivationPolicy_(
            NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
        );
        app.activateIgnoringOtherApps_(cocoa::base::YES);
        setup_app_menu(app);
        info!("macOS window activation and menu configured");
    }
}
#[cfg(target_os = "macos")]
unsafe fn setup_app_menu(app: id) {
    let _pool = NSAutoreleasePool::new(nil);
    let main_menu = NSMenu::new(nil);
    main_menu.autorelease();
    let app_menu = NSMenu::new(nil);
    app_menu.autorelease();
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
    let separator = NSMenuItem::separatorItem(nil);
    app_menu.addItem_(separator);
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
#[cfg(target_os = "macos")]
pub fn setup_transparent_titlebar() {
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

        info!("macOS transparent titlebar configured");
    }
}
