import AppKit
import BaeKit
import SwiftUI
import os.log

private let menuCommandsLogger = Logger.bae("MainAppMenuCommands")

/// The existing library services a focused window exposes to app commands,
/// which act through them. What the menus show of them is read through
/// `MenuBar`, never from here.
@MainActor
final class MainAppMenuTarget {
    let playbackStore: PlaybackStore
    let configStore: ConfigStore
    let libraryStore: LibraryStore
    let uiStore: UiStore
    let library: Library
    let playback: Playback

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        libraryStore: LibraryStore,
        uiStore: UiStore,
        library: Library,
        playback: Playback
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.libraryStore = libraryStore
        self.uiStore = uiStore
        self.library = library
        self.playback = playback
    }
}

private struct MainAppMenuTargetKey: FocusedValueKey {
    typealias Value = MainAppMenuTarget
}

extension FocusedValues {
    var mainAppMenuTarget: MainAppMenuTarget? {
        get { self[MainAppMenuTargetKey.self] }
        set { self[MainAppMenuTargetKey.self] = newValue }
    }
}

struct ImportFolderButton: View {
    let uiStore: UiStore?

    var body: some View {
        Button("Import Folder...") {
            guard let uiStore else {
                preconditionFailure(
                    "Import Folder is disabled without an open library"
                )
            }
            uiStore.setImportFolderPickerPresented(true)
        }
        .keyboardShortcut("i", modifiers: .command)
        .disabled(uiStore == nil)
    }
}

struct CloseLibraryButton: View {
    let onClose: () -> Void
    let isEnabled: Bool

    var body: some View {
        Button("Close Library") {
            onClose()
        }
        .keyboardShortcut("w", modifiers: [.command, .shift])
        .disabled(!isEnabled)
    }
}

/// The body of the File → Open Library submenu: one item per library, the
/// active one marked with a leading checkmark. The first nine carry ⌘⇧1…⌘⇧9
/// so libraries can be switched without opening the menu.
struct OpenLibrarySubmenu: View {
    let libraries: [LibraryMenuEntry]
    let onOpen: (_ libraryId: String) -> Void

    private static let shortcutKeys: [KeyEquivalent] = [
        "1", "2", "3", "4", "5", "6", "7", "8", "9",
    ]

    var body: some View {
        if libraries.isEmpty {
            Button("No Libraries") {}
                .disabled(true)
        }
        else {
            ForEach(Array(libraries.enumerated()), id: \.element.id) {
                idx,
                lib in
                let button = Button {
                    onOpen(lib.id)
                } label: {
                    if !lib.canOpen {
                        // Listed, so it isn't lost — but it cannot be opened.
                        Label(lib.name, systemImage: "exclamationmark.triangle")
                    }
                    else if lib.isActive {
                        Label(lib.name, systemImage: "checkmark")
                    }
                    else {
                        Text(lib.name)
                    }
                }
                .disabled(!lib.canOpen)
                if idx < Self.shortcutKeys.count {
                    button.keyboardShortcut(
                        Self.shortcutKeys[idx],
                        modifiers: [.command, .shift]
                    )
                }
                else {
                    button
                }
            }
        }
    }
}

struct LibraryNavigationButton: View {
    let target: MainAppMenuTarget?

    var body: some View {
        Button("Library") {
            guard let target else {
                preconditionFailure(
                    "Library is disabled without an open library"
                )
            }
            target.uiStore.navigateToLibraryRoot()
        }
        .keyboardShortcut("1", modifiers: .command)
        .disabled(target == nil)
    }
}

struct ImportNavigationButton: View {
    let target: MainAppMenuTarget?

    var body: some View {
        Button("Import") {
            guard let target else {
                preconditionFailure(
                    "Import is disabled without an open library"
                )
            }
            target.uiStore.navigateToImport()
        }
        .keyboardShortcut("2", modifiers: .command)
        .disabled(target == nil)
    }
}

struct OpenStorageManagerButton: View {
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        Button("Storage Manager") {
            openWindow(id: "storage-manager")
        }
        .keyboardShortcut("0", modifiers: .command)
    }
}

/// One checkmarked button per library browser mode. The focused menu target
/// identifies the main window whose library section and mode should change.
struct LibraryModeCommandButtons: View {
    let target: MainAppMenuTarget
    let selected: LibraryBrowserMode

    var body: some View {
        LibraryModeButtons(selected: selected) { mode in
            target.uiStore.navigateToLibraryRoot()
            target.uiStore.setLibraryBrowserMode(mode)
        }
    }
}

/// The body of the Playback → Repeat submenu: one checkmarked item per mode,
/// each setting the mode absolutely. The active mode carries a leading
/// checkmark. The now-playing bar's single button cycles instead.
struct RepeatModeMenuItems: View {
    let current: BridgeRepeatMode?
    let onSelect: (BridgeRepeatMode) -> Void

    private static let items:
        [(mode: BridgeRepeatMode, title: LocalizedStringKey)] =
            [
                (.off, "Off"),
                (.context, "All"),
                (.track, "One"),
            ]

    var body: some View {
        ForEach(Array(Self.items.enumerated()), id: \.offset) { _, item in
            Button {
                onSelect(item.mode)
            } label: {
                if item.mode == current {
                    Label(item.title, systemImage: "checkmark")
                }
                else {
                    Text(item.title)
                }
            }
        }
    }
}

/// A Playback menu item for a preference that is on or off: it carries a
/// leading checkmark while the preference is on, and a click writes the
/// opposite of what the item shows rather than flipping whatever the store
/// holds by then.
struct PlaybackPreferenceMenuItem: View {
    let title: LocalizedStringKey
    let isOn: Bool
    let setEnabled: (Bool) -> Void

    var body: some View {
        Button {
            setEnabled(!isOn)
        } label: {
            if isOn {
                Label(title, systemImage: "checkmark")
            }
            else {
                Text(title)
            }
        }
    }
}

/// Send a standard edit action up the responder chain from the key window's
/// first responder, as the menu item AppKit would have built does.
@MainActor
private func sendToFirstResponder(_ action: Selector) {
    NSApp.sendAction(action, to: nil, from: nil)
}

extension FocusedValues {
    /// Select All for a focused list that holds only some of its rows: it
    /// selects every row the list shows, loaded or not.
    @Entry
    var selectAllShownRows: FocusedCommand?
}

/// Every command bae adds to the main menu.
///
/// What the items show comes from `menuBar.state`, which holds still while a
/// menu is open; nothing here reads a store or a user default for display.
/// The focused values say which window's library and views the commands act
/// on, and are objects that live as long as the view publishing them. The
/// actions act on the live services when chosen, which may be after what the
/// open menu showed has moved on.
struct MainAppMenuCommands: Commands {
    let menuBar: MenuBar
    /// The library lifecycle the File menu drives, and the updater.
    let app: AppDelegate
    @FocusedValue(\.mainAppMenuTarget)
    private var target
    @FocusedValue(\.focusSearch)
    private var focusSearch
    @FocusedValue(\.selectAllShownRows)
    private var selectAllShownRows

    var body: some Commands {
        let state = menuBar.state
        // The open library's state, where the key window is one of its.
        let library = target == nil ? nil : state.library

        CommandGroup(after: .appInfo) {
            Button("Check for Updates...") {
                app.requiredApplicationServices.checkForUpdatesViewModel
                    .checkForUpdates()
            }
            .disabled(!state.canCheckForUpdates)
        }

        // File commands use native placements and disable when no library
        // is open. Close Library precedes `.saveItem`, which retains the
        // native Close item.
        CommandGroup(after: .newItem) {
            Button("New Library...") { app.presentWelcome(mode: nil) }
                .keyboardShortcut("n", modifiers: [.command, .option])
            Button("Join a Library...") { app.presentWelcome(mode: .join(nil)) }
            Button("Restore from Code...") {
                app.presentWelcome(mode: .restore)
            }
            Menu("Open Library") {
                OpenLibrarySubmenu(libraries: state.libraries) {
                    app.openLocalLibrary(id: $0)
                }
                Divider()
                Button("Previous Library") { app.switchLibrary(byOffset: -1) }
                    .keyboardShortcut("[", modifiers: [.command, .shift])
                    .disabled(target == nil)
                Button("Next Library") { app.switchLibrary(byOffset: 1) }
                    .keyboardShortcut("]", modifiers: [.command, .shift])
                    .disabled(target == nil)
            }
        }
        CommandGroup(after: .importExport) {
            ImportFolderButton(uiStore: target?.uiStore)
        }
        CommandGroup(before: .saveItem) {
            Button("Rename Library...") { app.presentRenameLibrary() }
                .disabled(target == nil)
            Button("Lock Library...") {
                app.presentLockLibraryConfirmation()
            }
            .disabled(target == nil)
            Button("Sync Now") { app.syncNow() }
                .disabled(target == nil)
            Button("Reveal Library in Finder") { app.revealLibraryInFinder() }
                .disabled(target == nil)
            Button("Copy Library ID") { app.copyLibraryId() }
                .disabled(target == nil)
            CloseLibraryButton(
                onClose: { app.closeLibrary() },
                isEnabled: target != nil
            )
        }

        // The Edit menu's clipboard items, owned so Select All reaches a list
        // whose table holds only its loaded rows. Each other item sends its
        // standard action to the first responder and is enabled while that
        // responder can take it, as AppKit's own are.
        CommandGroup(replacing: .pasteboard) {
            responderButton("Cut", #selector(NSText.cut(_:)), state)
                .keyboardShortcut("x")
            responderButton("Copy", #selector(NSText.copy(_:)), state)
                .keyboardShortcut("c")
            responderButton("Paste", #selector(NSText.paste(_:)), state)
                .keyboardShortcut("v")
            responderButton(
                "Paste and Match Style",
                #selector(NSTextView.pasteAsPlainText(_:)),
                state
            )
            .keyboardShortcut("v", modifiers: [.command, .option, .shift])
            responderButton("Delete", #selector(NSText.delete(_:)), state)
            Button("Select All") {
                if let selectAllShownRows {
                    selectAllShownRows.send()
                }
                else {
                    sendToFirstResponder(#selector(NSText.selectAll(_:)))
                }
            }
            .keyboardShortcut("a")
            .disabled(
                selectAllShownRows == nil
                    && !state.clipboard.contains(
                        #selector(NSText.selectAll(_:))
                    )
            )
        }

        CommandGroup(before: .toolbar) {
            LibraryNavigationButton(target: target)
            ImportNavigationButton(target: target)
            OpenStorageManagerButton()

            if let target, let library {
                Divider()
                LibraryModeCommandButtons(
                    target: target,
                    selected: library.browserMode
                )
                Divider()
                // Writes through the library services installed in the
                // focused window.
                Toggle(
                    "Full-Width Library",
                    isOn: Binding(
                        get: { library.fullWidth },
                        set: { enabled in
                            Task {
                                do {
                                    try await target.library
                                        .setLibraryFullWidth(enabled)
                                }
                                catch {
                                    target.uiStore.showError(error)
                                }
                            }
                        }
                    )
                )
                Divider()
            }

            Button("Search") {
                guard let focusSearch else {
                    preconditionFailure(
                        "Search is disabled without a focused search field"
                    )
                }
                focusSearch.send()
            }
            .keyboardShortcut("/", modifiers: [])
            .disabled(focusSearch == nil)

            Divider()

            Button("Go to Now Playing") {
                goToNowPlaying()
            }
            .keyboardShortcut("l", modifiers: .command)
            .disabled(library?.hasNowPlayingTrack != true)

            Button("Toggle Queue") {
                let target = requireTarget()
                target.uiStore.setQueuePresented(!target.uiStore.showQueue)
            }
            .keyboardShortcut("s", modifiers: [.command, .shift])
            .disabled(target == nil)

            Divider()
        }

        CommandMenu("Playback") {
            Button("Play / Pause") {
                let target = requireTarget()
                target.playback.playPause(for: target.playbackStore.nowPlaying)
            }
            .keyboardShortcut(.space, modifiers: [])
            .disabled(target == nil)

            Button("Next Track") {
                requireTarget().playback.nextTrack()
            }
            .keyboardShortcut(.rightArrow, modifiers: [.command, .option])
            .disabled(target == nil)

            Button("Previous Track") {
                requireTarget().playback.previousTrack()
            }
            .keyboardShortcut(.leftArrow, modifiers: [.command, .option])
            .disabled(target == nil)

            Button("Mute") {
                let target = requireTarget()
                target.playback.setMuted(!target.playbackStore.isMuted)
            }
            .keyboardShortcut("m", modifiers: [.command, .option])
            .disabled(target == nil)

            Divider()

            Button("Cycle Repeat Mode") {
                let target = requireTarget()
                target.playback.setRepeatMode(
                    bridgeNextRepeatMode(mode: target.playbackStore.repeatMode)
                )
            }
            .keyboardShortcut("r", modifiers: .command)
            .disabled(target == nil)

            Menu("Repeat") {
                RepeatModeMenuItems(current: library?.repeatMode) { mode in
                    requireTarget().playback.setRepeatMode(mode)
                }
            }
            .disabled(target == nil)

            Divider()

            // The Playback settings pane's two toggles, reachable without
            // opening settings. Both write the same places the pane does.
            PlaybackPreferenceMenuItem(
                title: "Pause between sides and discs",
                isOn: library?.pausesBetweenSides == true
            ) { enabled in
                let target = requireTarget()
                Task {
                    do {
                        try await target.playback.setPauseBetweenSides(enabled)
                    }
                    catch {
                        target.uiStore.showError(error)
                    }
                }
            }
            .disabled(target == nil)

            PlaybackPreferenceMenuItem(
                title: "Restore on launch",
                isOn: state.restoresPlaybackOnLaunch
            ) { enabled in
                UserDefaults.standard.set(enabled, forKey: "persistPlayback")
            }

            Divider()

            Button("Shuffle Library") {
                requireTarget().playback.playLibraryShuffled()
            }
            .disabled(library?.canShuffle != true)
        }
    }

    /// An item that sends `action` to the first responder, enabled while the
    /// responder can take it.
    private func responderButton(
        _ title: LocalizedStringKey,
        _ action: Selector,
        _ state: MenuBarState
    ) -> some View {
        Button(title) { sendToFirstResponder(action) }
            .disabled(!state.clipboard.contains(action))
    }

    private func requireTarget() -> MainAppMenuTarget {
        guard let target else {
            preconditionFailure("Library command invoked without its target")
        }
        return target
    }

    /// The item shows whether a track was playing when its menu opened;
    /// playback may have stopped by the time it is chosen.
    private func goToNowPlaying() {
        let target = requireTarget()
        let navigation = NowPlayingNavigationAction(
            playbackStore: target.playbackStore,
            uiStore: target.uiStore
        )
        guard navigation.isEnabled else {
            menuCommandsLogger.info(
                "Go to Now Playing chosen after playback stopped; nothing to go to"
            )
            return
        }
        navigation.perform()
    }
}
