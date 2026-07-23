import BaeKit

extension BridgeSavePreset {
    /// Whether the Track scope reads as on. A single-file+CUE image is a
    /// whole-release export, so Track can never be on for it.
    var trackScopeOn: Bool {
        pregapPlacement != .singleFileWithCue && appliesToTrack
    }

    /// Whether the Release scope reads as on. A single-file+CUE image is a
    /// whole-release export, so Release always reads on for it.
    var releaseScopeOn: Bool {
        pregapPlacement == .singleFileWithCue || appliesToRelease
    }

    /// This preset with Track applicability set to `on`, honoring the
    /// single-file+CUE rule that forbids a track scope.
    func withTrackScope(_ on: Bool) -> BridgeSavePreset {
        var changed = self
        changed.appliesToTrack = on && pregapPlacement != .singleFileWithCue
        return changed
    }

    /// This preset with Release applicability set to `on`, honoring the
    /// single-file+CUE rule that forces a release scope.
    func withReleaseScope(_ on: Bool) -> BridgeSavePreset {
        var changed = self
        changed.appliesToRelease = on || pregapPlacement == .singleFileWithCue
        return changed
    }
}
