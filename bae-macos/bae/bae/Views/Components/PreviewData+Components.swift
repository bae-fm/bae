#if DEBUG
    import BaeKit
    import SwiftUI

    // Fixtures for the Components leaf previews. Extends the shared `PreviewData`
    // namespace so the component previews draw their sample values from one place.
    @MainActor
    extension PreviewData {
        /// A UI-originated failure — prose the UI already localized, with no opaque
        /// detail to disclose. Renders the plain line with no disclosure row.
        static let displayErrorSimple = DisplayError(
            line:
                "Couldn't reach the sync service. Check your connection and try again."
        )

        /// A core diagnostic crossing the bridge: its category renders the generic
        /// line and the opaque Rust error chain rides along as copyable `detail`, so
        /// the disclosure row appears.
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

        /// A rip whose weakest track thirty-seven other people's copies
        /// agree with — two tracks, each confirmed by both databases.
        static var releaseVerification: BridgeVerification {
            BridgeVerification(
                source: .log,
                matchedCopies: 37,
                tracks: [
                    BridgeTrackVerification(
                        number: 1,
                        accurateripConfidence: 42,
                        ctdbConfidence: 16,
                        crc: 0xE94F_69D5
                    ),
                    BridgeTrackVerification(
                        number: 2,
                        accurateripConfidence: 37,
                        ctdbConfidence: 16,
                        crc: 0xBF12_B7A9
                    ),
                ]
            )
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
#endif
