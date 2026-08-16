import BaeKit
import SwiftUI

/// The existing library services a focused window exposes to app commands.
/// Commands receive live stores instead of copying their current values.
@MainActor
final class MainAppMenuTarget {
    let playbackStore: PlaybackStore
    let configStore: ConfigStore
    let libraryStore: LibraryStore
    let importStore: ImportStore
    let uiStore: UiStore
    let library: Library
    let playback: Playback
    let importer: Importer

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        libraryStore: LibraryStore,
        importStore: ImportStore,
        uiStore: UiStore,
        library: Library,
        playback: Playback,
        importer: Importer
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.libraryStore = libraryStore
        self.importStore = importStore
        self.uiStore = uiStore
        self.library = library
        self.playback = playback
        self.importer = importer
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

/// File commands use native placements and disable when no library is open.
/// Close Library precedes `.saveItem`, which retains the native Close item.
struct LibraryFileMenuCommands: Commands {
    @FocusedValue(\.mainAppMenuTarget)
    private var target
    let libraries: [BridgeLibrary]
    let onNewLibrary: (WelcomeView.Mode?) -> Void
    let onOpenLibrary: (BridgeLibrary) -> Void
    let onSwitchOffset: (Int) -> Void
    let onRenameLibrary: () -> Void
    let onLockLibrary: () -> Void
    let onSyncNow: () -> Void
    let onRevealLibrary: () -> Void
    let onCopyLibraryId: () -> Void
    let onCloseLibrary: () -> Void

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Button("New Library...") { onNewLibrary(nil) }
                .keyboardShortcut("n", modifiers: [.command, .option])
            Button("Join a Library...") { onNewLibrary(.join) }
            Button("Restore from Code...") { onNewLibrary(.restore) }
            Menu("Open Library") {
                OpenLibrarySubmenu(libraries: libraries, onOpen: onOpenLibrary)
                Divider()
                Button("Previous Library") { onSwitchOffset(-1) }
                    .keyboardShortcut("[", modifiers: [.command, .shift])
                    .disabled(target == nil)
                Button("Next Library") { onSwitchOffset(1) }
                    .keyboardShortcut("]", modifiers: [.command, .shift])
                    .disabled(target == nil)
            }
        }
        CommandGroup(after: .importExport) {
            ImportFolderButton(uiStore: target?.uiStore)
        }
        CommandGroup(before: .saveItem) {
            Button("Rename Library...") { onRenameLibrary() }
                .disabled(target == nil)
            Button("Lock Library...") { onLockLibrary() }
                .disabled(target == nil)
            Button("Sync Now") { onSyncNow() }
                .disabled(target == nil)
            Button("Reveal Library in Finder") { onRevealLibrary() }
                .disabled(target == nil)
            Button("Copy Library ID") { onCopyLibraryId() }
                .disabled(target == nil)
            CloseLibraryButton(
                onClose: onCloseLibrary,
                isEnabled: target != nil
            )
        }
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
    let target: MainAppMenuTarget?
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        Button("Library") {
            guard let target else {
                preconditionFailure(
                    "Library is disabled without an open library"
                )
            }
            openWindow(id: MainWindow.sceneID)
            target.uiStore.navigateToLibraryRoot()
        }
        .keyboardShortcut("1", modifiers: .command)
        .disabled(target == nil)
    }
}

struct OpenImportButton: View {
    let target: MainAppMenuTarget?
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        Button("Import") {
            guard let target else {
                preconditionFailure(
                    "Import is disabled without an open library"
                )
            }
            openWindow(id: MainWindow.sceneID)
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

/// One checkmarked button per library browser mode. Opens the main window and
/// navigates to the library section before setting the mode, so the item works
/// from any window (matching `OpenLibraryButton`).
struct LibraryModeCommandButtons: View {
    let target: MainAppMenuTarget
    @Environment(\.openWindow)
    private var openWindow

    var body: some View {
        LibraryModeButtons(uiStore: target.uiStore) { mode in
            openWindow(id: MainWindow.sceneID)
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

struct MainAppMenuCommands: Commands {
    @FocusedValue(\.mainAppMenuTarget)
    private var target
    @FocusedValue(\.focusSearch)
    var focusSearch

    var body: some Commands {
        CommandGroup(after: .pasteboard) {
            Button("Skip All") {
                let action = requireImportCandidateSkipAction()
                Task { await action.perform() }
            }
            .keyboardShortcut("e", modifiers: .command)
            .disabled(!canSkipSelectedImportCandidates)
        }

        CommandGroup(before: .toolbar) {
            OpenLibraryButton(target: target)
            OpenImportButton(target: target)
            OpenStorageManagerButton()

            if let target {
                Divider()
                LibraryModeCommandButtons(target: target)
                Divider()
                // Reads the live config and writes through the same library
                // services installed in the focused window.
                Toggle(
                    "Full-Width Library",
                    isOn: Binding(
                        get: { target.configStore.config.libraryFullWidth },
                        set: { enabled in
                            do {
                                try target.library.setLibraryFullWidth(enabled)
                            }
                            catch {
                                target.uiStore.showError(error)
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
                focusSearch()
            }
            .keyboardShortcut("/", modifiers: [])
            .disabled(focusSearch == nil)

            Divider()

            Button("Go to Now Playing") {
                goToNowPlaying()
            }
            .keyboardShortcut("l", modifiers: .command)
            .disabled(target?.playbackStore.nowPlaying.track?.albumId == nil)

            Button("Toggle Queue") {
                requireTarget().uiStore.toggleQueue()
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
                RepeatModeMenuItems(
                    current: target?.playbackStore.repeatMode
                ) { mode in
                    requireTarget().playback.setRepeatMode(mode)
                }
            }
            .disabled(target == nil)

            Divider()

            Button("Shuffle Library") {
                requireTarget().playback.playLibraryShuffled()
            }
            .disabled(!canShuffleLibrary)
        }
    }

    private func requireTarget() -> MainAppMenuTarget {
        guard let target else {
            preconditionFailure("Library command invoked without its target")
        }
        return target
    }

    private func requireImportCandidateSkipAction()
        -> ImportCandidateSkipAction
    {
        let target = requireTarget()
        return ImportCandidateSkipAction(
            importer: target.importer,
            importStore: target.importStore,
            uiStore: target.uiStore
        )
    }

    private var canSkipSelectedImportCandidates: Bool {
        guard let target, case .importing = target.uiStore.activeSection else {
            return false
        }
        return ImportCandidateSkipAction(
            importer: target.importer,
            importStore: target.importStore,
            uiStore: target.uiStore
        )
        .isEnabled
    }

    private var canShuffleLibrary: Bool {
        guard let albumTotal = target?.libraryStore.albumTotal else {
            return false
        }
        return albumTotal > 0
    }

    private func goToNowPlaying() {
        let target = requireTarget()
        guard let albumId = target.playbackStore.nowPlaying.track?.albumId
        else {
            preconditionFailure(
                "Go to Now Playing is disabled without a playing album"
            )
        }
        let trackId = target.playbackStore.nowPlaying.track?.trackId
        // Store an override only when the playing track's release is not the
        // album default; unloaded details leave the default unchanged.
        let releaseId: String? = {
            guard let trackId,
                let summary = target.libraryStore.albumSummaries[albumId]
            else {
                return nil
            }
            let matchingReleaseId = summary.releaseIds.first { id in
                target.libraryStore.releaseDetails[id]?.tracks
                    .contains(where: { $0.id == trackId }) ?? false
            }
            guard let matchingReleaseId else {
                return nil
            }
            return matchingReleaseId == summary.primaryReleaseId
                ? nil : matchingReleaseId
        }()
        target.uiStore.navigateToAlbum(
            albumId,
            trackId: trackId,
            releaseId: releaseId
        )
    }
}
