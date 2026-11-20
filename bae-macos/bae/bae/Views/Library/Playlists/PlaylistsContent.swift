import BaeKit
import SwiftUI

/// The side pane's Playlists tab: every playlist this device holds, each a
/// place to drop tracks, or the open playlist's page.
struct PlaylistsContent: View {
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Group {
            if let playlistId = uiStore.openPlaylistId {
                MacPlaylistPage(detail: openDetail(playlistId))
            }
            else if let listing = libraryProjections.playlists.value {
                PlaylistList(listing: listing)
            }
            else if let error = libraryProjections.playlists.error {
                MacNotice(
                    title: String(localized: "Could not read your library"),
                    advice: nil,
                    error: error,
                    look: .centered,
                    action: nil
                )
            }
            else {
                // The first value is a local read; nothing shows until it
                // lands.
                Color.clear
            }
        }
        .detailRead(
            id: uiStore.openPlaylistId,
            activate: libraryProjections.activatePlaylist,
            deactivate: libraryProjections.deactivatePlaylist
        )
    }

    /// The open playlist's page as the read last delivered it, once the read
    /// has delivered that playlist.
    private func openDetail(_ playlistId: String) -> BridgePlaylistDetail? {
        let detail = libraryProjections.playlist.value
        return detail?.summary.id == playlistId ? detail : nil
    }
}

/// The playlists by name, each opening its page in the pane and taking the
/// tracks dropped on it.
private struct PlaylistList: View {
    let listing: BridgePlaylistListing

    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryBrowseSession.self)
    private var session
    /// The playlist the dragged tracks are over.
    @State
    private var dropTarget: String?

    var body: some View {
        if listing.total == 0 {
            PlaceholderTitle(
                listing.canCreate
                    ? "No playlists"
                    : "Playlists appear once you have a profile"
            )
        }
        else {
            ScrollView {
                LazyVStack(spacing: ThemeSpace.hairline) {
                    ForEach(listing.listed, id: \.id) { playlist in
                        row(playlist)
                    }
                }
                .padding(.horizontal, ThemeSpace.related)
            }
            .scrollIndicators(.never)
        }
    }

    private func row(_ playlist: BridgePlaylistSummary) -> some View {
        MacLibraryRowButton(
            playlistCover: playlist.cover,
            title: playlist.name,
            subtitle: FactList.joined([
                playlist.trackCount.localizedLine,
                playlist.sharing.label,
            ]),
            dropping: dropTarget == playlist.id,
            action: { uiStore.openPlaylist(playlist.id) }
        )
        // Only the person's own playlists change.
        .dropDestination(for: String.self) { dropped, _ in
            guard playlist.yours, !dropped.isEmpty else { return false }
            append(dropped, to: playlist.id)
            return true
        } isTargeted: { over in
            if over, playlist.yours {
                dropTarget = playlist.id
            }
            else if dropTarget == playlist.id {
                dropTarget = nil
            }
        }
    }

    /// Add the tracks `dropped` names to the end of the playlist
    /// `playlistId`, and show why when that fails.
    private func append(_ dropped: [String], to playlistId: String) {
        let library = library
        let uiStore = uiStore
        let order = AlbumGridOrder(session: session)
        Task {
            do {
                let trackIds = try await DroppedTracks.trackIds(
                    dropped,
                    order: order,
                    in: library
                )
                guard !trackIds.isEmpty else { return }
                try await library.playlistWrites.appendPicked(
                    playlistId,
                    trackIds
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
