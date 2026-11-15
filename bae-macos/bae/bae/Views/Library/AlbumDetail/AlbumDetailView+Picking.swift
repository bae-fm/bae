import BaeKit

extension AlbumDetailView {
    /// What the album's track menus do with the picked tracks: each action
    /// hands core the ids and acts on what core answers, the tracks in the
    /// order their release lists them.
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

    /// Call `apply` with `trackIds` in the order their release lists them,
    /// which core reads; a failed read says why and calls nothing. A lone
    /// track needs no read.
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
