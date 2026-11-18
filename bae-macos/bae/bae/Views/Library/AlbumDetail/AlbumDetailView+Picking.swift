import BaeKit

extension AlbumDetailView {
    /// What the album's track menus do with the picked tracks. Each action
    /// sends the picked track ids to core in no order, and core plays or
    /// queues them in the order their release lists them.
    var trackPicking: TrackPicking {
        TrackPicking(
            onExtend: { targetId in
                uiStore.pickedTracks.extend(
                    to: targetId,
                    readRange: library.trackIdsBetween,
                    onFailure: uiStore.showError
                )
            },
            onPlay: { trackIds in playback.playPickedTracks(trackIds) },
            onAddNext: { trackIds in queue.addPickedNext(trackIds) },
            onAddToQueue: { trackIds in queue.addPickedToQueue(trackIds) }
        )
    }
}
