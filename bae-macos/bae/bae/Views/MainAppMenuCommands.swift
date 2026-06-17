import AppKit
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
            panel.message = "Select a folder to watch for music to import"
            panel.prompt = "Add"
            guard panel.runModal() == .OK, let url = panel.url else {
                return
            }
            do {
                try importer.addWatchedFolder(url.path)
                uiStore.navigateToImport()
            }
            catch {
                uiStore.showError(
                    "Couldn't add folder: \(error.localizedDescription)"
                )
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

struct SwitchLibraryButton: View {
    let onSwitch: () -> Void

    var body: some View {
        Button("Switch Library...") {
            onSwitch()
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

struct MainAppMenuCommands: Commands {
    let playback: Playback
    let importer: Importer
    let libraryStore: LibraryStore
    let playbackStore: PlaybackStore
    let uiStore: UiStore
    let onCloseLibrary: () -> Void
    let onSwitchLibrary: () -> Void
    @FocusedValue(\.focusSearch)
    var focusSearch

    var body: some Commands {
        CommandGroup(after: .newItem) {
            ImportFolderButton(importer: importer, uiStore: uiStore)
            Divider()
            SwitchLibraryButton(onSwitch: onSwitchLibrary)
            CloseLibraryButton(onClose: onCloseLibrary)
        }

        CommandGroup(before: .toolbar) {
            OpenLibraryButton(uiStore: uiStore)
            OpenImportButton(uiStore: uiStore)
            OpenStorageManagerButton()

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
                playback.togglePlayPause()
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

            Divider()

            Button("Cycle Repeat Mode") {
                playback.cycleRepeatMode()
            }
            .keyboardShortcut("r", modifiers: .command)
        }
    }
}
