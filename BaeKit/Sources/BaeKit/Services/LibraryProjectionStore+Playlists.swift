import Foundation

extension LibraryProjectionStore {
    public func activatePlaylist(_ id: String) {
        if playlistReader?.id != id { playlist = LibraryProjectionState() }
        playlistReader =
            playlistReader
            ?? detailReader(
                open: library.playlistDetail,
                state: \.playlist
            )
        playlistReader?.show(id)
    }

    public func deactivatePlaylist(_ id: String) {
        guard playlistReader?.id == id else { return }
        playlistReader?.close()
    }

    /// Start reading every playlist, unless it is read already. Both the
    /// Playlists view and a track's Add to playlist menu need the list, so
    /// the read stays open for the library's life.
    public func activatePlaylists() {
        guard playlistDeliveries == nil else { return }
        playlistDeliveries = ScreenSubscription(
            library.playlists(),
            onValue: { [weak self] listing in
                self?.playlists = LibraryProjectionState(
                    value: listing,
                    delivered: true
                )
            },
            onError: { [weak self] error in
                guard let self else { return }
                playlists = LibraryProjectionState(
                    value: playlists.value,
                    delivered: playlists.delivered,
                    error: DisplayError(error)
                )
            }
        )
    }
}
