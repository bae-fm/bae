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

/// Runs work on the main actor once the change that asked for it has landed.
typealias MenuBarScheduler =
    @Sendable (@escaping @MainActor @Sendable () -> Void) -> Void

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
/// while one is, and catches up when the last one closes — as AppKit
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
/// `read` gathers the state. It runs again when anything it read under
/// observation changes, when AppKit updates its windows (after every event
/// it handles — what the first responder can do is asked, not observed), and
/// when a user default changes.
@MainActor
@Observable
final class MenuBar {
    /// What the commands render.
    private(set) var state: MenuBarState

    @ObservationIgnored
    private let read: @MainActor () -> MenuBarState
    @ObservationIgnored
    private let schedule: MenuBarScheduler
    /// How many menus are being tracked now; `state` holds while any is.
    @ObservationIgnored
    private var openMenus = 0
    /// Whether the observable values the last read went through are still
    /// watched. Observation reports one change and stops, so the next read
    /// after a change watches them again, and reads before one — on AppKit's
    /// updates — need not.
    @ObservationIgnored
    private var watching = false

    init(
        notifications: NotificationCenter = .default,
        schedule: @escaping MenuBarScheduler = { work in
            Task { @MainActor in work() }
        },
        read: @escaping @MainActor () -> MenuBarState
    ) {
        self.read = read
        self.schedule = schedule
        state = read()
        // Never removed: the app holds one of these for its whole life.
        _ = notifications.addObserver(
            forName: NSMenu.didBeginTrackingNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.openMenus += 1 }
        }
        _ = notifications.addObserver(
            forName: NSMenu.didEndTrackingNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.menuClosed() }
        }
        _ = notifications.addObserver(
            forName: NSApplication.didUpdateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        _ = notifications.addObserver(
            forName: UserDefaults.didChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        refresh()
    }

    private func menuClosed() {
        if openMenus == 0 {
            menuBarLogger.warning(
                "A menu ended tracking that never began; reading the menu bar again"
            )
        }
        else {
            openMenus -= 1
        }
        refresh()
    }

    /// Observation reports a change as it is about to be made, so the read
    /// that shows it waits until it has landed.
    private func inputChanged() {
        watching = false
        refresh()
    }

    private func refresh() {
        guard openMenus == 0 else { return }
        let now: MenuBarState
        if watching {
            now = read()
        }
        else {
            watching = true
            now = withObservationTracking {
                read()
            } onChange: { [weak self, schedule] in
                schedule { self?.inputChanged() }
            }
        }
        // Only a different value is written, so the commands are asked
        // again only when what they show has changed.
        if now != state {
            state = now
        }
    }
}
