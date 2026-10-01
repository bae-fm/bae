import AppKit
import BaeKit
import Observation
import os.log

private let menuBarLogger = Logger.bae("MenuBar")

/// What the menu bar shows: every changing value its commands render, read
/// together by `MenuBar`.
///
/// A copy rather than the stores themselves, because an open menu has to keep
/// showing what it opened with while the stores move on — a track ends, the
/// library list reloads, a sync rewrites the config. SwiftUI rebuilds the main
/// menu's menus whenever anything their commands read changes, and AppKit
/// throws when that happens to a menu on screen (`NSMenu setItemArray` while
/// it is tracked). What the commands read can only hold still if it is held
/// apart from what keeps moving.
struct MenuBarState: Equatable {
    let canCheckForUpdates: Bool
    /// Every library on this device, for File ▸ Open Library.
    let libraries: [LibraryMenuEntry]
    /// Which of the Edit menu's clipboard actions the first responder can
    /// take.
    let clipboard: Set<Selector>
    /// The "Restore on launch" preference.
    let restoresPlaybackOnLaunch: Bool
    /// The open library's, or nil while none is open.
    let library: LibraryMenuState?
}

/// One library in File ▸ Open Library.
struct LibraryMenuEntry: Equatable {
    let id: String
    let name: String
    let isActive: Bool
    /// False for a library whose config could not be read: it is listed, so it
    /// isn't lost, but it cannot be opened.
    let canOpen: Bool

    init(_ library: BridgeLibrary) {
        id = library.id
        name = library.name
        isActive = library.isActive
        canOpen = library.error == nil
    }
}

/// What the menus show of the open library.
struct LibraryMenuState: Equatable {
    let browserMode: LibraryBrowserMode
    let fullWidth: Bool
    /// Whether a track is playing, for View ▸ Go to Now Playing.
    let hasNowPlayingTrack: Bool
    let repeatMode: BridgeRepeatMode
    let pausesBetweenSides: Bool
    /// Whether the library has any album to shuffle.
    let canShuffle: Bool

    @MainActor
    init(reading target: MainAppMenuTarget) {
        browserMode = target.uiStore.libraryBrowserMode
        fullWidth = target.configStore.config.libraryFullWidth
        hasNowPlayingTrack =
            NowPlayingNavigationAction(
                playbackStore: target.playbackStore,
                uiStore: target.uiStore
            )
            .isEnabled
        repeatMode = target.playbackStore.repeatMode
        pausesBetweenSides = target.configStore.config.pauseBetweenSides
        if let albumTotal = target.libraryStore.albumTotal {
            canShuffle = albumTotal > 0
        }
        else {
            // Not counted yet.
            canShuffle = false
        }
    }
}

/// The one place the menu bar's commands read anything that changes, held
/// still while a menu is open.
///
/// SwiftUI updates the main menu's menus whenever what their commands read
/// changes, open or not, and AppKit throws when a menu on screen has its
/// items replaced (`NSMenu setItemArray` while tracking, failing in
/// `NSContextMenuImpl preferredViewHeightForMenuItemAtIndex`). An update that
/// changes nothing visible is no safer: SwiftUI replaces a menu's items
/// whenever their count differs from the menu's, and AppKit adds items of
/// its own to some menus, Edit among them, so for those it always does.
///
/// So the commands read `state` and nothing else that changes: it follows
/// its inputs while no menu is open, holds what the open menu was built from
/// while one is, and catches up once the last one closes — as AppKit
/// validates its own items when their menu opens and holds them while it is
/// open. A command that read a store, a user default or anything else that
/// moves directly would bring the crash back; its value belongs in
/// `MenuBarState`. The commands' own values are only stable references, so
/// rebuilding them changes nothing either.
///
/// The key window's focused values reach the menus without passing through
/// here — a change to them updates every menu — so every value published
/// for the commands is one object for the publishing view's lifetime
/// (`MainAppMenuTarget`, `FocusedCommand`), never one rebuilt on each
/// render.
///
/// `read` gathers the state at the end of every pass of the main run loop,
/// just before SwiftUI renders what changed in it (SwiftUI's own run loop
/// observer, order 0, before waiting and on exit). Whatever moved the
/// inputs — an event to a window, a menu command sent by its key
/// equivalent, a task landing, a user default — it ran in that pass, so
/// nothing has to say that it happened: AppKit posts no window update after
/// a key equivalent, and what the first responder can do is asked, never
/// observed. Writing ahead of SwiftUI's render means the commands are
/// rebuilt in the same pass, never left pending for a later pass in which a
/// menu may have opened. A change made during that render, such as focus
/// moving to a field a command asked for, is read again after it and wakes
/// the run loop for one more pass, so it is shown before the loop sleeps.
@MainActor
@Observable
final class MenuBar {
    /// What the commands render.
    private(set) var state: MenuBarState

    @ObservationIgnored
    private let read: @MainActor () -> MenuBarState
    /// How many menus are being tracked now; `state` holds while any is.
    @ObservationIgnored
    private var openMenus = 0
    @ObservationIgnored
    private let subscriptions: Subscriptions

    /// Reads on the passes of `runLoop` in `mode`: the main run loop in
    /// every common mode — menus are tracked, and modal panels run, in
    /// their own — unless a test runs one of its own. `wakeUp` asks
    /// `runLoop` for another pass.
    init(
        notifications: NotificationCenter = .default,
        runLoop: CFRunLoop = CFRunLoopGetMain(),
        mode: CFRunLoopMode = .commonModes,
        wakeUp: @escaping @MainActor (CFRunLoop) -> Void = {
            CFRunLoopWakeUp($0)
        },
        read: @escaping @MainActor () -> MenuBarState
    ) {
        self.read = read
        state = read()
        subscriptions = Subscriptions(notifications: notifications)
        subscriptions.observe(NSMenu.didBeginTrackingNotification) {
            [weak self] in
            self?.openMenus += 1
        }
        subscriptions.observe(NSMenu.didEndTrackingNotification) {
            [weak self] in
            self?.menuClosed()
        }
        let passEnds: CFRunLoopActivity = [.beforeWaiting, .exit]
        subscriptions.observe(
            runLoop,
            mode,
            passEnds,
            order: Self.beforeSwiftUIRenders
        ) { [weak self] in
            self?.refresh()
        }
        subscriptions.observe(
            runLoop,
            mode,
            passEnds,
            order: Self.afterEverything
        ) { [weak self] in
            guard let self, self.isStale else { return }
            wakeUp(runLoop)
        }
    }

    /// SwiftUI renders on its run loop observer of order 0.
    private static let beforeSwiftUIRenders: CFIndex = -1
    private static let afterEverything: CFIndex = .max

    private func menuClosed() {
        if openMenus == 0 {
            menuBarLogger.warning("A menu ended tracking that never began")
        }
        else {
            openMenus -= 1
        }
    }

    private func refresh() {
        guard openMenus == 0 else { return }
        let now = read()
        // Only a different value is written, so the commands are asked
        // again only when what they show has changed.
        if now != state {
            state = now
        }
    }

    /// Whether the next pass would show something new.
    private var isStale: Bool {
        openMenus == 0 && read() != state
    }
}

/// The notification and run loop observers a `MenuBar` installed, removed
/// with it.
private final class Subscriptions: @unchecked Sendable {
    private let notifications: NotificationCenter
    private var tokens: [NSObjectProtocol] = []
    private var observers: [RunLoopObserver] = []

    private struct RunLoopObserver {
        let observer: CFRunLoopObserver
        let runLoop: CFRunLoop
        let mode: CFRunLoopMode
    }

    init(notifications: NotificationCenter) {
        self.notifications = notifications
    }

    /// Calls `handle` for each `name` posted on the main thread.
    func observe(
        _ name: Notification.Name,
        _ handle: @escaping @MainActor () -> Void
    ) {
        tokens.append(
            notifications.addObserver(
                forName: name,
                object: nil,
                queue: .main
            ) { _ in
                MainActor.assumeIsolated(handle)
            }
        )
    }

    /// Calls `handle` at each of `activities` of `runLoop`, a run loop of the
    /// main thread, in `mode`, among that activity's observers by `order`.
    func observe(
        _ runLoop: CFRunLoop,
        _ mode: CFRunLoopMode,
        _ activities: CFRunLoopActivity,
        order: CFIndex,
        _ handle: @escaping @MainActor () -> Void
    ) {
        let observer = CFRunLoopObserverCreateWithHandler(
            nil,
            activities.rawValue,
            true,
            order
        ) { _, _ in
            MainActor.assumeIsolated(handle)
        }
        guard let observer else {
            preconditionFailure("CFRunLoopObserverCreateWithHandler failed")
        }
        CFRunLoopAddObserver(runLoop, observer, mode)
        observers.append(
            RunLoopObserver(observer: observer, runLoop: runLoop, mode: mode)
        )
    }

    deinit {
        for token in tokens {
            notifications.removeObserver(token)
        }
        for installed in observers {
            CFRunLoopRemoveObserver(
                installed.runLoop,
                installed.observer,
                installed.mode
            )
            CFRunLoopObserverInvalidate(installed.observer)
        }
    }
}
