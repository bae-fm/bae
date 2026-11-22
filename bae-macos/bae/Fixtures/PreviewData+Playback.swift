import BaeKit

@testable import bae

@MainActor
extension PreviewData {
    /// A seek bar's track at `positionMs` of `durationMs`, as its stream
    /// would hold it.
    static func trackPosition(
        positionMs: Int64,
        durationMs: UInt64,
        progress: Double? = nil
    ) -> LatestSource<BridgeTrackPosition> {
        LatestStream.trackPositions(
            holding: [
                "preview-track": BridgeTrackPosition(
                    positionMs: positionMs,
                    durationMs: durationMs,
                    progress: progress ?? Double(positionMs)
                        / Double(durationMs),
                    seeks: 0
                )
            ]
        )
        .source(for: "preview-track")
    }

    /// The parts of the preview track held by the same track's buffer.
    static func trackBuffered(_ ranges: [BridgeTrackSpan])
        -> LatestSource<BridgeTrackBuffered>
    {
        LatestStream.trackBuffered(
            holding: ["preview-track": BridgeTrackBuffered(ranges: ranges)]
        )
        .source(for: "preview-track")
    }
}
