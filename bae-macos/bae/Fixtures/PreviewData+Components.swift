import BaeKit
import SwiftUI

@testable import bae

// Fixtures for the component shots.
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

    /// A shot clip at `positionMs` of `durationMs`, as its stream
    /// would hold it.
    static func previewPosition(
        positionMs: UInt64,
        durationMs: UInt64
    ) -> LatestSource<BridgePreviewPosition> {
        LatestStream.previewPosition(
            holding: BridgePreviewPosition(
                target: BridgePreviewTarget(
                    path: URL(
                        fileURLWithPath:
                            "/Music/Downloads/Album Title/01 Track Title.flac"
                    )
                    .pathBytes,
                    name: "01 Track Title.flac",
                    startSample: 0,
                    endSample: nil
                ),
                durationMs: durationMs,
                positionMs: positionMs,
                progress: Double(positionMs) / Double(durationMs),
                seeks: 0
            )
        )
        .only
    }

    /// A core diagnostic: a generic line and a copyable detail.
    static let displayErrorWithDetail: DisplayError = {
        guard
            let error = DisplayError(
                BridgeError.Diagnostic(
                    category: .database,
                    detail: "no such table: releases (code 1)"
                ) as any Error
            )
        else {
            fatalError("a Diagnostic error always yields a DisplayError")
        }
        return error
    }()

    /// A failure a second try can get past: a core line with a detail, from
    /// a call that reached no one.
    static let displayErrorRetryable = DisplayError(
        line: "Couldn’t reach the cloud.",
        detail: "timed out connecting to the library's storage"
    )
}

// The record fixtures stay off the main actor: the library fixtures that
// give a release its records are built there too.
extension PreviewData {
    /// A release the two asked catalogs describe — what an import that
    /// paired a MusicBrainz release with a Discogs one commits.
    static var releaseRecordsPair: [BridgeReleaseRecord] {
        [
            record(.musicBrainz, "mb-release-1"),
            record(.discogs, "424242"),
        ]
    }
}
