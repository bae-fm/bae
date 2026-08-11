import AppKit
import BaeKit
import SwiftUI

struct ImportFolderButton: View {
    let importer: Importer
    let uiStore: UiStore

    var body: some View {
        Button("Import Folder...") {
            let panel = NSOpenPanel()
            panel.canCreateDirectories = false
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.allowsMultipleSelection = false
            panel.message = String(
                localized: "Select a folder to watch for music to import"
            )
            panel.prompt = String(localized: "Add")
            guard panel.runModal() == .OK, let url = panel.url else {
                return
            }
            Task {
                do {
                    try await importer.addWatchedFolder(url.path)
                    uiStore.navigateToImport()
                }
                catch {
                    uiStore.showError(
                        String(
                            localized:
                                "Couldn't add folder: \(error.displayLine)"
                        )
                    )
                }
            }
        }
        .keyboardShortcut("i", modifiers: .command)
    }
}

struct CloseLibraryButton: View {
    let onClose: () -> Void

    var body: some View {
        Button("Close Library") {
            onClose()
        }
        .keyboardShortcut("w", modifiers: [.command, .shift])
    }
}

/// The body of the File → Open Library submenu: one item per library, the
/// active one marked with a leading checkmark. The first nine carry ⌘⇧1…⌘⇧9
/// so libraries can be switched without opening the menu.
struct OpenLibrarySubmenu: View {
    let libraries: [BridgeLibrary]
    let onOpen: (BridgeLibrary) -> Void

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
                    onOpen(lib)
                } label: {
                    if lib.error != nil {
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
                .disabled(lib.error != nil)
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

struct OpenLibraryButton: View {
    let uiStore: UiStore
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        Button("Library") {
            openWindow(id: "main")
            uiStore.navigateToLibraryRoot()
        }
        .keyboardShortcut("1", modifiers: .command)
    }
}

struct OpenImportButton: View {
    let uiStore: UiStore
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        Button("Import") {
            openWindow(id: "main")
            uiStore.navigateToImport()
        }
        .keyboardShortcut("2", modifiers: .command)
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

/// One checkmarked button per library browser mode. Opens the main window and
/// navigates to the library section before setting the mode, so the item works
/// from any window (matching `OpenLibraryButton`).
struct LibraryModeCommandButtons: View {
    let uiStore: UiStore
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        LibraryModeButtons(uiStore: uiStore) { mode in
            openWindow(id: "main")
            uiStore.navigateToLibraryRoot()
            uiStore.setLibraryBrowserMode(mode)
        }
    }
}

/// The body of the Playback → Repeat submenu: one checkmarked item per mode,
/// each setting the mode absolutely. The active mode carries a leading
/// checkmark. The now-playing bar's single button cycles instead.
struct RepeatModeMenuItems: View {
    let current: BridgeRepeatMode
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

struct MainAppMenuCommands: Commands {
    @Environment(Playback.self)
    private var playback
    @Environment(Importer.self)
    private var importer
    @Environment(Library.self)
    private var library
    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(ConfigStore.self)
    private var configStore
    /// Every library on this device, for the Open Library submenu. The active
    /// one is marked; the rest are switch targets.
    let libraries: [BridgeLibrary]
    /// Present the welcome flow at the given mode (`nil` = the default chooser,
    /// `.restore` = restore-from-code, `.join` = join-a-library).
    let onNewLibrary: (WelcomeView.Mode?) -> Void
    let onOpenLibrary: (BridgeLibrary) -> Void
    /// Switch to the library `offset` positions from the active one (wraps).
    let onSwitchOffset: (Int) -> Void
    let onRenameLibrary: () -> Void
    let onLockLibrary: () -> Void
    let onSyncNow: () -> Void
    let onRevealLibrary: () -> Void
    let onCopyLibraryId: () -> Void
    let onCloseLibrary: () -> Void
    @FocusedValue(\.focusSearch)
    var focusSearch

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("New Library...") { onNewLibrary(nil) }
                .keyboardShortcut("n", modifiers: [.command, .option])
            Button("Join a Library...") { onNewLibrary(.join) }
            Button("Restore from Code...") { onNewLibrary(.restore) }

            Menu("Open Library") {
                OpenLibrarySubmenu(
                    libraries: libraries,
                    onOpen: onOpenLibrary
                )
                Divider()
                Button("Previous Library") { onSwitchOffset(-1) }
                    .keyboardShortcut("[", modifiers: [.command, .shift])
                Button("Next Library") { onSwitchOffset(1) }
                    .keyboardShortcut("]", modifiers: [.command, .shift])
            }

            Divider()
            ImportFolderButton(importer: importer, uiStore: uiStore)
            Divider()
            Button("Rename Library...") { onRenameLibrary() }
            Button("Lock Library...") { onLockLibrary() }
            Button("Sync Now") { onSyncNow() }
            Button("Reveal Library in Finder") { onRevealLibrary() }
            Button("Copy Library ID") { onCopyLibraryId() }
            Divider()
            CloseLibraryButton(onClose: onCloseLibrary)
        }

        CommandGroup(before: .toolbar) {
            OpenLibraryButton(uiStore: uiStore)
            OpenImportButton(uiStore: uiStore)
            OpenStorageManagerButton()

            Divider()

            LibraryModeCommandButtons(uiStore: uiStore)

            Divider()

            // Reads the synced `libraryFullWidth` config; the write goes
            // through the bridge, and the resulting config invalidation
            // refreshes `configStore` — which is what re-checks this item.
            Toggle(
                "Full-Width Library",
                isOn: Binding(
                    get: { configStore.config.libraryFullWidth },
                    set: { enabled in
                        do {
                            try library.setLibraryFullWidth(enabled)
                        }
                        catch {
                            uiStore.showError(error)
                        }
                    }
                )
            )

            Divider()

            Button("Search") {
                focusSearch?()
            }
            .keyboardShortcut("/", modifiers: [])

            Divider()

            if let albumId = playbackStore.nowPlaying.track?.albumId {
                Button("Go to Now Playing") {
                    let trackId = playbackStore.nowPlaying.track?.trackId
                    // Only store a per-album override when the playing track's
                    // release isn't the album's primary — primary is the default.
                    // Looks through already-loaded release details; if none match,
                    // leaves releaseId nil and the detail view picks the default.
                    let releaseId: String? = {
                        guard let tId = trackId,
                            let summary = libraryStore.albumSummaries[albumId]
                        else {
                            return nil
                        }
                        let matchingReleaseId = summary.releaseIds.first { id in
                            libraryStore.releaseDetails[id]?.tracks
                                .contains(where: {
                                    $0.id == tId
                                }) ?? false
                        }
                        guard let matchingReleaseId else {
                            return nil
                        }
                        return matchingReleaseId == summary.primaryReleaseId
                            ? nil : matchingReleaseId
                    }()
                    uiStore.navigateToAlbum(
                        albumId,
                        trackId: trackId,
                        releaseId: releaseId
                    )
                }
                .keyboardShortcut("l", modifiers: .command)
            }

            Button("Toggle Queue") {
                uiStore.toggleQueue()
            }
            .keyboardShortcut("s", modifiers: [.command, .shift])

            Divider()
        }

        CommandMenu("Playback") {
            Button("Play / Pause") {
                playback.playPause(for: playbackStore.nowPlaying)
            }
            .keyboardShortcut(.space, modifiers: [])

            Button("Next Track") {
                playback.nextTrack()
            }
            .keyboardShortcut(.rightArrow, modifiers: [.command, .option])

            Button("Previous Track") {
                playback.previousTrack()
            }
            .keyboardShortcut(.leftArrow, modifiers: [.command, .option])

            Button("Mute") {
                playback.setMuted(!playbackStore.isMuted)
            }
            .keyboardShortcut("m", modifiers: [.command, .option])

            Divider()

            Button("Cycle Repeat Mode") {
                playback.setRepeatMode(
                    bridgeNextRepeatMode(mode: playbackStore.repeatMode)
                )
            }
            .keyboardShortcut("r", modifiers: .command)

            Menu("Repeat") {
                RepeatModeMenuItems(current: playbackStore.repeatMode) { mode in
                    playback.setRepeatMode(mode)
                }
            }

            Divider()

            Button("Shuffle Library") {
                playback.playLibraryShuffled()
            }
            .disabled((libraryStore.albumTotal ?? 0) == 0)
        }
    }
}
