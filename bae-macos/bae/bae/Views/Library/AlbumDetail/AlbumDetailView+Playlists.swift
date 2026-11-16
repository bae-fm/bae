import BaeKit

extension AlbumDetailView {
    /// The playlists a track's menu can add to, and what choosing one does.
    var trackPlaylistChoices: TrackPlaylistChoices {
        TrackPlaylistChoices(
            listing: libraryProjections?.playlists.value,
            onAdd: addToPlaylist,
            onNewPlaylist: { trackIds in
                inReleaseOrder(trackIds) { ordered in
                    uiStore.presentNewPlaylist(trackIds: ordered)
                }
            }
        )
    }

    /// Add `trackIds` to the end of the playlist `playlistId`, in their
    /// release's order; if the write fails, the view shows the error and adds
    /// no tracks.
    private func addToPlaylist(trackIds: [String], playlistId: String) {
        let library = library
        inReleaseOrder(trackIds) { ordered in
            Task {
                do {
                    try await library.playlistWrites.append(
                        playlistId,
                        ordered
                    )
                }
                catch {
                    uiStore.showError(error)
                }
            }
        }
    }
}
