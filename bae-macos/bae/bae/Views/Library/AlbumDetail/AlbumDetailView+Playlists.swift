import BaeKit

extension AlbumDetailView {
    /// The playlists a track's menu can add to, and what choosing one does.
    var trackPlaylistChoices: TrackPlaylistChoices {
        TrackPlaylistChoices(
            listing: libraryProjections?.playlists.value,
            onAdd: addToPlaylist,
            onNewPlaylist: { trackIds in
                uiStore.presentNewPlaylist(pickedTrackIds: trackIds)
            }
        )
    }

    /// Add the picked `trackIds` to the end of the playlist `playlistId`, in
    /// their release's order; if the write fails, the view shows the error and
    /// adds no tracks.
    private func addToPlaylist(trackIds: [String], playlistId: String) {
        let library = library
        let uiStore = uiStore
        // The write is the person's change to the playlist, so it finishes
        // even after the album page closes; nothing holds the task.
        Task {
            do {
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
