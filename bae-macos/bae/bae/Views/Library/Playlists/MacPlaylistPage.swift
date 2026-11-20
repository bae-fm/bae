import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("MacPlaylistPage")

/// A playlist's page inside the side pane's Playlists tab: its picture beside
/// its name, owner and facts over Play and its ⋯ menu, then its tracks in
/// order.
struct MacPlaylistPage: View {
    /// The playlist as its read last delivered it; nil until the read of the
    /// open playlist delivers it.
    let detail: BridgePlaylistDetail?

    @Environment(Playback.self)
    private var playback
    @Environment(Queue.self)
    private var queue
    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryBrowseSession.self)
    private var session

    @State
    private var editing: EditPlaylistSheetState?
    @State
    private var deleting: BridgePlaylistSummary?

    /// The width of the column the entries' numbers hang in, left of the
    /// picture's edge.
    static let numberWidth: CGFloat = 20

    var body: some View {
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
                        move: { await move($0, to: $1, in: detail.summary) },
                        insert: {
                            await insert($0, at: $1, in: detail.summary)
                        }
                    )
                }
                // The open playlist's read is local: nothing shows until it
                // lands.
            }
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.section)
            // The margin holds the entries' numbers, which hang left of the
            // picture's edge.
            .padding(
                .horizontal,
                ThemeSpace.related
                    + MacListRow.hang(leadWidth: Self.numberWidth)
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollIndicators(.never)
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

    /// Delete `playlist` and go back to the list when it is open. When the
    /// delete fails, show the error.
    private func delete(_ playlist: BridgePlaylistSummary) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                try await library.playlistWrites.delete(playlist.id)
                uiStore.closePlaylist(playlist.id)
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

    /// Put the tracks `dropped` names at `place` in `playlist`, an album's in
    /// the grid's order, and show the error when the write fails.
    private func insert(
        _ dropped: [String],
        at place: BridgeBesideEntry,
        in playlist: BridgePlaylistSummary
    ) async {
        do {
            let trackIds = try await DroppedTracks.trackIds(
                dropped,
                order: AlbumGridOrder(session: session),
                in: library
            )
            guard !trackIds.isEmpty else {
                logger.warning(
                    "a drop on playlist \(playlist.id) held no tracks: \(dropped.joined(separator: ", "))"
                )
                return
            }
            try await library.playlistWrites.insertPicked(
                playlist.id,
                trackIds,
                place
            )
        }
        catch {
            uiStore.showError(error)
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
