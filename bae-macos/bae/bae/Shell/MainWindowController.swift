import AppKit
import BaeKit
import Combine
import SwiftUI
import os.log

private let mainWindowLogger = Logger.bae("MainWindowController")

/// The primary window, and the main menu's commands for the library it shows.
///
/// The window's controller is in its responder chain while the window is key
/// or main, so these commands reach it, and are enabled, only then: the
/// window whose library they act on is the one the person is using. Each is
/// enabled from what its library holds when AppKit asks, as the menu opens
/// or its key equivalent is pressed.
@MainActor
final class MainWindowController: HostingWindowController, NSMenuItemValidation
{
    private unowned let appDelegate: AppDelegate
    /// The open library's services while the window shows it.
    private let library: @MainActor () -> MainAppMenuTarget?
    /// View ▸ Search's requests, which the title bar's field takes focus on.
    private let searchFocusRequests: PassthroughSubject<Void, Never>

    init(
        appDelegate: AppDelegate,
        library: @escaping @MainActor () -> MainAppMenuTarget?,
        onClose: @escaping () -> Void
    ) {
        self.appDelegate = appDelegate
        self.library = library
        let searchFocusRequests = PassthroughSubject<Void, Never>()
        self.searchFocusRequests = searchFocusRequests
        super
            .init(
                content: MainWindowRoot(
                    appDelegate: appDelegate,
                    searchFocusRequests: searchFocusRequests
                ),
                contentSize: MainWindow.sizeOnMainScreen,
                // The content sets the window's size limits: fixed while
                // bootstrap shows, the library's minimum once it opens.
                sizing: [.minSize, .maxSize],
                bridging: [],
                onClose: onClose
            )
        guard let window else {
            preconditionFailure("the primary window controller has its window")
        }
        // The content reaches under the title bar, which shows only the
        // window buttons.
        window.styleMask.insert(.fullSizeContentView)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        // The window has no title; AppKit's default is "Untitled".
        window.title = ""
    }

    private var target: MainAppMenuTarget? {
        library()
    }

    private func requireTarget() -> MainAppMenuTarget {
        guard let target else {
            preconditionFailure("Library command invoked without its library")
        }
        return target
    }

    /// Whether the window shows the open library, title bar and all.
    private var showsLibrary: Bool {
        guard target != nil, case .library = appDelegate.screen else {
            return false
        }
        return true
    }

    // MARK: - Validation

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if let checked = isChecked(menuItem) {
            menuItem.state = checked ? .on : .off
        }
        return isEnabled(menuItem)
    }

    /// Whether a checkmarked item shows its checkmark; nil for an item that
    /// has none.
    private func isChecked(_ item: NSMenuItem) -> Bool? {
        switch item.action {
        case #selector(chooseLibraryBrowserMode(_:)):
            target?.uiStore.libraryBrowserMode
                == LibraryBrowserMode.allCases[item.tag]
        case #selector(fullWidthLibrary(_:)):
            target?.configStore.config.libraryFullWidth == true
        case #selector(chooseRepeatMode(_:)):
            target?.playbackStore.repeatMode == repeatModeChoice(item).mode
        case #selector(pauseBetweenSides(_:)):
            target?.configStore.config.pauseBetweenSides == true
        default:
            nil
        }
    }

    private func isEnabled(_ item: NSMenuItem) -> Bool {
        guard let target else { return false }
        switch item.action {
        case #selector(renameLibrary(_:)):
            return target.membershipStore.selfIsAdmin
        case #selector(focusSearch(_:)):
            return showsLibrary
        case #selector(goToNowPlaying(_:)):
            return nowPlayingNavigation(target).isEnabled
        case #selector(shuffleLibrary(_:)):
            // No total while the library's albums are not counted yet.
            guard let albumTotal = target.libraryBrowseSession.albumTotal else {
                return false
            }
            return albumTotal > 0
        default:
            return true
        }
    }

    // MARK: - File

    @objc
    func previousLibrary(_: Any?) {
        appDelegate.switchLibrary(byOffset: -1)
    }

    @objc
    func nextLibrary(_: Any?) {
        appDelegate.switchLibrary(byOffset: 1)
    }

    @objc
    func importFolder(_: Any?) {
        requireTarget().uiStore.setImportFolderPickerPresented(true)
    }

    @objc
    func renameLibrary(_: Any?) {
        appDelegate.presentRenameLibrary()
    }

    @objc
    func syncNow(_: Any?) {
        appDelegate.syncNow()
    }

    @objc
    func revealLibraryInFinder(_: Any?) {
        appDelegate.revealLibraryInFinder()
    }

    @objc
    func copyLibraryId(_: Any?) {
        appDelegate.copyLibraryId()
    }

    @objc
    func closeLibrary(_: Any?) {
        appDelegate.closeLibrary()
    }

    // MARK: - View

    @objc
    func showLibrarySection(_: Any?) {
        requireTarget().uiStore.navigateToLibraryRoot()
    }

    @objc
    func showImportSection(_: Any?) {
        requireTarget().uiStore.navigateToImport()
    }

    @objc
    func chooseLibraryBrowserMode(_ sender: NSMenuItem) {
        let uiStore = requireTarget().uiStore
        uiStore.navigateToLibraryRoot()
        uiStore.setLibraryBrowserMode(LibraryBrowserMode.allCases[sender.tag])
    }

    /// Sets the opposite of what the item showed when its menu opened.
    @objc
    func fullWidthLibrary(_ sender: NSMenuItem) {
        let target = requireTarget()
        let enabled = sender.state != .on
        Task {
            do {
                try await target.library.setLibraryFullWidth(enabled)
            }
            catch {
                target.uiStore.showError(error)
            }
        }
    }

    @objc
    func focusSearch(_: Any?) {
        searchFocusRequests.send()
    }

    /// The item shows whether a track was playing when its menu opened;
    /// playback may have stopped by the time it is chosen.
    @objc
    func goToNowPlaying(_: Any?) {
        let navigation = nowPlayingNavigation(requireTarget())
        guard navigation.isEnabled else {
            mainWindowLogger.info(
                "Go to Now Playing chosen after playback stopped; nothing to go to"
            )
            return
        }
        navigation.perform()
    }

    @objc
    func toggleQueue(_: Any?) {
        let uiStore = requireTarget().uiStore
        uiStore.setQueuePresented(!uiStore.showQueue)
    }

    // MARK: - Playback

    @objc
    func playPause(_: Any?) {
        let target = requireTarget()
        target.playback.playPause(for: target.playbackStore.nowPlaying)
    }

    @objc
    func nextTrack(_: Any?) {
        requireTarget().playback.nextTrack()
    }

    @objc
    func previousTrack(_: Any?) {
        requireTarget().playback.previousTrack()
    }

    @objc
    func mute(_: Any?) {
        let target = requireTarget()
        target.playback.setMuted(!target.playbackStore.isMuted)
    }

    @objc
    func cycleRepeatMode(_: Any?) {
        let target = requireTarget()
        target.playback.setRepeatMode(
            bridgeNextRepeatMode(mode: target.playbackStore.repeatMode)
        )
    }

    @objc
    func chooseRepeatMode(_ sender: NSMenuItem) {
        requireTarget().playback.setRepeatMode(repeatModeChoice(sender).mode)
    }

    /// Sets the opposite of what the item showed when its menu opened, as
    /// the Playback settings pane's toggle writes it.
    @objc
    func pauseBetweenSides(_ sender: NSMenuItem) {
        let target = requireTarget()
        let enabled = sender.state != .on
        Task {
            do {
                try await target.playback.setPauseBetweenSides(enabled)
            }
            catch {
                target.uiStore.showError(error)
            }
        }
    }

    @objc
    func shuffleLibrary(_: Any?) {
        requireTarget().playback.playLibraryShuffled()
    }

    // MARK: - Helpers

    private func nowPlayingNavigation(
        _ target: MainAppMenuTarget
    ) -> NowPlayingNavigationAction {
        NowPlayingNavigationAction(
            playbackStore: target.playbackStore,
            uiStore: target.uiStore
        )
    }

    private func repeatModeChoice(_ item: NSMenuItem) -> RepeatModeChoice {
        guard let choice = RepeatModeChoice(rawValue: item.tag) else {
            preconditionFailure("a Repeat item's tag names a repeat mode")
        }
        return choice
    }
}
