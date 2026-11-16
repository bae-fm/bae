import BaeKit

extension AlbumDetailView {
    /// What the album's track menus do with the picked tracks. Each action
    /// sends the picked track ids to core. Core returns the tracks in the
    /// order their release lists them, and the action then plays or queues
    /// them.
    var trackPicking: TrackPicking {
        TrackPicking(
            picked: uiStore.pickedTracks,
            onExtend: extendPicks(to:),
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

    /// Pick the tracks from the anchor to `targetId`, which core reads; with
    /// no anchor, pick and anchor `targetId` alone. A failed read says why
    /// and picks nothing.
    func extendPicks(to targetId: String) {
        let picked = uiStore.pickedTracks
        guard let anchorId = picked.anchorId else {
            picked.set(targetId, picked: true)
            return
        }
        let library = library
        let uiStore = uiStore
        Task { @MainActor in
            do {
                picked.extend(
                    over: try await library.trackIdsBetween(anchorId, targetId)
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    /// Call `apply` with `trackIds` in the order their release lists them.
    /// Core reads the release order. If that read fails, `inReleaseOrder`
    /// shows the error and does not call `apply`. For a single track, the
    /// function calls `apply` without the read.
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
