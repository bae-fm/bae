import BaeKit

extension AlbumDetailView {
    /// The playlists a track's menu can add to, and what choosing one does.
    var trackPlaylistChoices: TrackPlaylistChoices {
        TrackPlaylistChoices(
            listing: libraryProjections?.playlists.value,
            onAdd: addToPlaylist,
            onNewPlaylist: { trackId in
                uiStore.presentNewPlaylist(trackIds: [trackId])
            }
        )
    }

    /// Add `trackId` to the end of the playlist `playlistId`; a failure says
    /// why.
    private func addToPlaylist(trackId: String, playlistId: String) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                try await library.appendPlaylistEntry(playlistId, trackId)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
