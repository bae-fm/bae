use bae_core::playback::RepeatMode;
use cocoa::appkit::{
    NSApplication, NSApplicationActivationPolicy, NSEventModifierFlags, NSMenu, NSMenuItem,
    NSWindow, NSWindowStyleMask, NSWindowTitleVisibility,
};
use cocoa::base::{id, nil, selector, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use dispatch::Queue;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use tracing::info;

static MENU_HANDLER_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();
static MENU_DELEGATE_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();
static UPDATE_MENU_ITEM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static REPEAT_MENU_ITEM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static REPEAT_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

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
unsafe fn register_menu_handler_class() {
    MENU_HANDLER_CLASS_REGISTERED.call_once(|| {
        use crate::ui::shortcuts::{
            request_nav, request_playback_action, NavAction, NavTarget, PlaybackAction,
        };

        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("BaeMenuHandler", superclass).unwrap();

        extern "C" fn check_for_updates(_this: &Object, _cmd: Sel, _sender: id) {
            crate::updater::check_for_updates();
        }

        extern "C" fn go_back(_this: &Object, _cmd: Sel, _sender: id) {
            request_nav(NavAction::Back);
        }

        extern "C" fn go_forward(_this: &Object, _cmd: Sel, _sender: id) {
            request_nav(NavAction::Forward);
        }

        extern "C" fn go_library(_this: &Object, _cmd: Sel, _sender: id) {
            request_nav(NavAction::GoTo(NavTarget::Library));
        }

        extern "C" fn go_import(_this: &Object, _cmd: Sel, _sender: id) {
            request_nav(NavAction::GoTo(NavTarget::Import));
        }

        extern "C" fn toggle_repeat_mode(_this: &Object, _cmd: Sel, _sender: id) {
            let current = REPEAT_MODE.load(std::sync::atomic::Ordering::SeqCst);
            let next = match current {
                0 => RepeatMode::Track,
                1 => RepeatMode::Album,
                _ => RepeatMode::None,
            };
            let next_value = match next {
                RepeatMode::None => 0,
                RepeatMode::Track => 1,
                RepeatMode::Album => 2,
            };
            REPEAT_MODE.store(next_value, std::sync::atomic::Ordering::SeqCst);
            unsafe {
                update_repeat_menu_state_inner();
            }

            request_playback_action(PlaybackAction::SetRepeatMode(next));
        }

        extern "C" fn toggle_play_pause(_this: &Object, _cmd: Sel, _sender: id) {
            request_playback_action(PlaybackAction::TogglePlayPause);
        }

        extern "C" fn next_track(_this: &Object, _cmd: Sel, _sender: id) {
            request_playback_action(PlaybackAction::Next);
        }

        extern "C" fn previous_track(_this: &Object, _cmd: Sel, _sender: id) {
            request_playback_action(PlaybackAction::Previous);
        }

        decl.add_method(
            sel!(checkForUpdates:),
            check_for_updates as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(goBack:), go_back as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(goForward:),
            go_forward as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(goLibrary:),
            go_library as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(goImport:),
            go_import as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(toggleRepeatMode:),
            toggle_repeat_mode as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(togglePlayPause:),
            toggle_play_pause as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(nextTrack:),
            next_track as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(previousTrack:),
            previous_track as extern "C" fn(&Object, Sel, id),
        );

        decl.register();
    });
}

/// Get or create the shared menu handler instance
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

                update_repeat_menu_state_inner();
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
pub fn setup_app_menu() {
    Queue::main().exec_async(|| unsafe {
        let app = NSApplication::sharedApplication(nil);
        setup_app_menu_inner(app);
    });
}

pub fn set_playback_repeat_mode(mode: RepeatMode) {
    let value = match mode {
        RepeatMode::None => 0,
        RepeatMode::Track => 1,
        RepeatMode::Album => 2,
    };
    REPEAT_MODE.store(value, std::sync::atomic::Ordering::SeqCst);

    Queue::main().exec_async(|| unsafe {
        update_repeat_menu_state_inner();
    });
}

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

    // Hide bae
    let hide_title = NSString::alloc(nil).init_str("Hide bae");
    let hide_key = NSString::alloc(nil).init_str("h");
    let hide_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        hide_title,
        selector("hide:"),
        hide_key,
    );
    hide_item.autorelease();
    app_menu.addItem_(hide_item);

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

    // Go menu
    let go_menu = NSMenu::new(nil);
    go_menu.autorelease();
    let go_menu_title = NSString::alloc(nil).init_str("Go");
    let _: () = msg_send![go_menu, setTitle: go_menu_title];

    let menu_handler = get_menu_handler();

    let command_only = NSEventModifierFlags::NSCommandKeyMask;
    let command_shift =
        NSEventModifierFlags::NSCommandKeyMask | NSEventModifierFlags::NSShiftKeyMask;
    let no_modifiers = NSEventModifierFlags::empty();

    // Back
    let back_title = NSString::alloc(nil).init_str("Back");
    let back_key = NSString::alloc(nil).init_str("\u{F702}");
    let back_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        back_title,
        selector("goBack:"),
        back_key,
    );
    back_item.autorelease();
    let _: () = msg_send![back_item, setTarget: menu_handler];
    let _: () = msg_send![back_item, setKeyEquivalentModifierMask: command_only];
    go_menu.addItem_(back_item);

    // Forward
    let forward_title = NSString::alloc(nil).init_str("Forward");
    let forward_key = NSString::alloc(nil).init_str("\u{F703}");
    let forward_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        forward_title,
        selector("goForward:"),
        forward_key,
    );
    forward_item.autorelease();
    let _: () = msg_send![forward_item, setTarget: menu_handler];
    let _: () = msg_send![forward_item, setKeyEquivalentModifierMask: command_only];
    go_menu.addItem_(forward_item);

    let go_separator = NSMenuItem::separatorItem(nil);
    go_menu.addItem_(go_separator);

    // Library
    let library_title = NSString::alloc(nil).init_str("Library");
    let library_key = NSString::alloc(nil).init_str("1");
    let library_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        library_title,
        selector("goLibrary:"),
        library_key,
    );
    library_item.autorelease();
    let _: () = msg_send![library_item, setTarget: menu_handler];
    go_menu.addItem_(library_item);

    // Import
    let import_title = NSString::alloc(nil).init_str("Import");
    let import_key = NSString::alloc(nil).init_str("2");
    let import_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        import_title,
        selector("goImport:"),
        import_key,
    );
    import_item.autorelease();
    let _: () = msg_send![import_item, setTarget: menu_handler];
    go_menu.addItem_(import_item);

    // Playback menu
    let playback_menu = NSMenu::new(nil);
    playback_menu.autorelease();
    let playback_menu_title = NSString::alloc(nil).init_str("Playback");
    let _: () = msg_send![playback_menu, setTitle: playback_menu_title];

    let menu_handler = get_menu_handler();

    // Play/Pause
    let play_pause_title = NSString::alloc(nil).init_str("Play/Pause");
    let play_pause_key = NSString::alloc(nil).init_str(" ");
    let play_pause_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        play_pause_title,
        selector("togglePlayPause:"),
        play_pause_key,
    );
    play_pause_item.autorelease();
    let _: () = msg_send![play_pause_item, setTarget: menu_handler];
    let _: () = msg_send![play_pause_item, setKeyEquivalentModifierMask: no_modifiers];
    playback_menu.addItem_(play_pause_item);

    // Next
    let next_title = NSString::alloc(nil).init_str("Next");
    let next_key = NSString::alloc(nil).init_str("n");
    let next_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        next_title,
        selector("nextTrack:"),
        next_key,
    );
    next_item.autorelease();
    let _: () = msg_send![next_item, setTarget: menu_handler];
    let _: () = msg_send![next_item, setKeyEquivalentModifierMask: command_shift];
    playback_menu.addItem_(next_item);

    // Previous
    let previous_title = NSString::alloc(nil).init_str("Previous");
    let previous_key = NSString::alloc(nil).init_str("b");
    let previous_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        previous_title,
        selector("previousTrack:"),
        previous_key,
    );
    previous_item.autorelease();
    let _: () = msg_send![previous_item, setTarget: menu_handler];
    let _: () = msg_send![previous_item, setKeyEquivalentModifierMask: command_shift];
    playback_menu.addItem_(previous_item);

    let playback_separator = NSMenuItem::separatorItem(nil);
    playback_menu.addItem_(playback_separator);

    // Repeat mode (cycles on click)
    let repeat_title = NSString::alloc(nil).init_str("Repeat: Off");
    let repeat_key = NSString::alloc(nil).init_str("r");
    let repeat_item = NSMenuItem::alloc(nil).initWithTitle_action_keyEquivalent_(
        repeat_title,
        selector("toggleRepeatMode:"),
        repeat_key,
    );
    repeat_item.autorelease();
    let _: () = msg_send![repeat_item, setTarget: menu_handler];
    let _: () = msg_send![repeat_item, setKeyEquivalentModifierMask: command_shift];
    playback_menu.addItem_(repeat_item);
    REPEAT_MENU_ITEM.store(repeat_item as usize, std::sync::atomic::Ordering::SeqCst);

    update_repeat_menu_state_inner();

    let playback_menu_item = NSMenuItem::new(nil);
    playback_menu_item.autorelease();
    playback_menu_item.setSubmenu_(playback_menu);
    main_menu.addItem_(playback_menu_item);

    let go_menu_item = NSMenuItem::new(nil);
    go_menu_item.autorelease();
    go_menu_item.setSubmenu_(go_menu);
    main_menu.addItem_(go_menu_item);

    app.setMainMenu_(main_menu);
}

/// Configure the window with transparent titlebar and native traffic lights.
/// This must be called after the window is created.
/// Dispatches to main thread since Cocoa UI operations require it.
pub fn setup_transparent_titlebar() {
    Queue::main().exec_async(|| {
        setup_transparent_titlebar_inner();
    });
}

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

unsafe fn update_repeat_menu_state_inner() {
    let mode_value = REPEAT_MODE.load(std::sync::atomic::Ordering::SeqCst);
    let repeat_ptr = REPEAT_MENU_ITEM.load(std::sync::atomic::Ordering::SeqCst) as *mut Object;
    if repeat_ptr.is_null() {
        return;
    }

    let title = match mode_value {
        1 => "Repeat: Song",
        2 => "Repeat: Album",
        _ => "Repeat: Off",
    };
    let title = NSString::alloc(nil).init_str(title);
    let _: () = msg_send![repeat_ptr, setTitle: title];
}
