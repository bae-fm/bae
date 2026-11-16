import BaeKit

extension AlbumDetailView {
    /// What the album's track menus do with the picked tracks. Each action
    /// sends the picked track ids to core, which returns the tracks in the
    /// order their release lists them, and then plays or queues them.
    var trackPicking: TrackPicking {
        TrackPicking(
            picked: uiStore.pickedTracks,
            onPlay: { trackIds in
                inReleaseOrder(trackIds) { playback.playTracks($0) }
            },
            onAddNext: { trackIds in
                inReleaseOrder(trackIds) { queue.addNext($0) }
            },
            onAddToQueue: { trackIds in
                inReleaseOrder(trackIds) { queue.addToQueue($0) }
            }
        )
    }

    /// Call `apply` with `trackIds` in the order their release lists them.
    /// Core reads the release order. If that read fails, this shows the error
    /// and does not call `apply`. A single track skips the read.
    func inReleaseOrder(
        _ trackIds: [String],
        then apply: @escaping @MainActor ([String]) -> Void
    ) {
        guard trackIds.count > 1 else {
            apply(trackIds)
            return
        }
        let library = library
        let uiStore = uiStore
        Task { @MainActor in
            do {
                let ordered = try await library.tracksInReleaseOrder(trackIds)
                guard !ordered.isEmpty else { return }
                apply(ordered)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
