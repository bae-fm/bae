#if DEBUG
    import BaeKit
    import SwiftUI

    // Fixtures for the component previews.
    @MainActor
    extension PreviewData {
        /// A seek bar's track at `positionMs` of `durationMs`, as its stream
        /// would hold it.
        static func trackPosition(
            positionMs: Int64,
            durationMs: UInt64
        ) -> LatestSource<BridgeTrackPosition> {
            LatestStream.trackPositions(
                holding: [
                    "preview-track": BridgeTrackPosition(
                        positionMs: positionMs,
                        durationMs: durationMs,
                        progress: Double(positionMs) / Double(durationMs),
                        seekRevision: 0
                    )
                ]
            )
            .source(for: "preview-track")
        }

        /// Where each preview queue entry is while it plays: a third of the
        /// way through, whichever plays.
        static func queuePositions() -> LatestStream<BridgeTrackPosition> {
            .trackPositions(
                holding: Dictionary(
                    queueEntries.map {
                        (
                            $0.trackId,
                            BridgeTrackPosition(
                                positionMs: 73_000,
                                durationMs: 214_000,
                                progress: 0.34,
                                seekRevision: 0
                            )
                        )
                    },
                    uniquingKeysWith: { first, _ in first }
                )
            )
        }

        /// A preview clip at `positionMs` of `durationMs`, as its stream
        /// would hold it.
        static func previewPosition(
            positionMs: UInt64,
            durationMs: UInt64
        ) -> LatestSource<BridgePreviewPosition> {
            LatestStream.previewPosition(
                holding: BridgePreviewPosition(
                    target: BridgePreviewTarget(
                        path:
                            "/Music/Downloads/Album Title/01 Track Title.flac",
                        startSample: 0,
                        endSample: nil
                    ),
                    durationMs: durationMs,
                    positionMs: positionMs,
                    progress: Double(positionMs) / Double(durationMs)
                )
            )
            .only
        }

        /// A failure the UI worded itself, with no detail to disclose.
        static let displayErrorSimple = DisplayError(
            line:
                "Couldn't reach the sync service. Check your connection and try again."
        )

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

        /// A release every catalog describes: what a MusicBrainz release with
        /// a full set of url-rels seeds.
        static var releaseRecordsEveryCatalog: [BridgeReleaseRecord] {
            bridgeCatalogs()
                .map { catalog in
                    record(
                        catalog,
                        "key-1"
                    )
                }
        }

        static func record(
            _ catalog: BridgeCatalog,
            _ key: String
        ) -> BridgeReleaseRecord {
            BridgeReleaseRecord(
                catalog: catalog,
                url: "https://example.test/\(key)"
            )
        }
    }

    #Preview("Error Detail Disclosure") {
        VStack(alignment: .leading, spacing: ThemeSpace.section) {
            ErrorDetailDisclosure(error: PreviewData.displayErrorWithDetail)
            ErrorDetailDisclosure(
                error: PreviewData.displayErrorSimple,
                tone: .warning
            )
            ErrorDetailDisclosure(
                error: PreviewData.displayErrorSimple,
                showIcon: false
            )
        }
        .padding(ThemeSpace.section)
        .frame(width: 440)
        .background(Theme.background)
    }
#endif
