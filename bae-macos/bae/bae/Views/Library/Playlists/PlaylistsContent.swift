import BaeKit
import SwiftUI

/// The Playlists mode: every playlist this device holds beside the open
/// one's page.
struct PlaylistsContent: View {
    let listing: BridgePlaylistListing?
    let error: DisplayError?
    let detail: BridgePlaylistDetail?
    let fullWidth: Bool

    @Environment(LibraryBrowseSession.self)
    private var session

    var body: some View {
        Group {
            if let listing {
                if listing.total == 0 {
                    PlaceholderTitle(
                        listing.canCreate
                            ? "No playlists"
                            : "Playlists appear once you have a profile"
                    )
                }
                else {
                    BrowseSplit {
                        list(listing)
                    } detail: {
                        PlaylistDetailPane(detail: detail)
                    }
                    .libraryContentContainer(fullWidth: fullWidth)
                }
            }
            else if let error {
                Notice(
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
    }

    private func list(_ listing: BridgePlaylistListing) -> some View {
        ScrollView {
            LazyVStack(spacing: ThemeSpace.hairline) {
                ForEach(listing.listed, id: \.id) { playlist in
                    MacLibraryRowButton(
                        playlistCover: playlist.cover,
                        title: playlist.name,
                        subtitle: FactList.joined([
                            playlist.trackCount.localizedLine,
                            playlist.sharing.label,
                        ]),
                        selected: session.selectedPlaylistId == playlist.id,
                        action: { session.selectPlaylist(playlist.id) }
                    )
                }
            }
            .padding(ThemeSpace.related)
        }
        .reportsHeaderScroll(id: "playlistList")
    }
}
