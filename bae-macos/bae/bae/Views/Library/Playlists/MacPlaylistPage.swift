import BaeKit
import SwiftUI

/// A playlist's page: its picture, name and owner over Play, then its tracks
/// in order.
struct MacPlaylistPage: View {
    let detail: BridgePlaylistDetail?

    @Environment(LibraryBrowseSession.self)
    private var session
    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue
    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var editing: EditPlaylistSheetState?
    @State
    private var deleting: BridgePlaylistSummary?

    var body: some View {
        if detail == nil, session.selectedPlaylistId == nil {
            // Nothing chosen: the placeholder fills the pane, centred.
            PlaceholderTitle("Playlists")
                .background(Theme.surface)
        }
        else {
            page
        }
    }

    private var page: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                if let detail {
                    MacPlaylistHeader(
                        summary: detail.summary,
                        onEdit: {
                            editing = EditPlaylistSheetState(detail.summary)
                        },
                        onDelete: { deleting = detail.summary }
                    )
                    PlaylistEntryList(
                        detail: detail,
                        play: { entry in
                            playback.playPlaylist(
                                detail.summary.id,
                                entry.positionId,
                                false
                            )
                        },
                        playNext: { queue.addNext([$0.trackId]) },
                        addToQueue: { queue.addToQueue([$0.trackId]) },
                        remove: { removeEntry($0, from: detail.summary) },
                        move: { await move($0, to: $1, in: detail.summary) }
                    )
                }
                // A selected playlist's read is local: nothing shows until it
                // lands.
            }
            .padding(ThemeSpace.section)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .reportsHeaderScroll(id: "playlistDetail")
        .background(Theme.surface)
        .sheet(item: $editing) { sheet in
            EditPlaylistSheet(
                // While the sheet closes, it shows the state it had before
                // `editing` became nil.
                state: Binding(
                    get: { editing ?? sheet },
                    set: { editing = $0 }
                ),
                onCancel: { editing = nil },
                onSave: { save($0) }
            )
        }
        .macAlert(item: $deleting) { playlist in
            DeletePlaylistConfirmation.alert(name: playlist.name) {
                delete(playlist)
            }
        }
    }

    /// Save the name and who sees `sheet`'s playlist in one write; when the
    /// write fails, the sheet stays open and shows the error.
    private func save(_ sheet: EditPlaylistSheetState) {
        let library = library
        Task {
            do {
                try await library.playlistWrites.edit(
                    sheet.playlistId,
                    sheet.name,
                    sheet.sharing
                )
                editing = nil
            }
            catch {
                editing?.error = DisplayError(error)
            }
        }
    }

    /// Delete `playlist` and clear the selection if it names that playlist.
    /// When the delete fails, show the error.
    private func delete(_ playlist: BridgePlaylistSummary) {
        let library = library
        let uiStore = uiStore
        let session = session
        Task {
            do {
                try await library.playlistWrites.delete(playlist.id)
                session.deselectPlaylist(playlist.id)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    private func removeEntry(
        _ entry: BridgePlaylistEntryRow,
        from playlist: BridgePlaylistSummary
    ) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                try await library.playlistWrites.removeEntry(
                    playlist.id,
                    entry.positionId
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Move the entry `positionId` to `place` in `playlist`, and show the
    /// error when the write fails.
    private func move(
        _ positionId: String,
        to place: BridgeEntryPlace,
        in playlist: BridgePlaylistSummary
    ) async {
        do {
            try await library.playlistWrites.moveEntry(
                playlist.id,
                positionId,
                place
            )
        }
        catch {
            uiStore.showError(error)
        }
    }
}
