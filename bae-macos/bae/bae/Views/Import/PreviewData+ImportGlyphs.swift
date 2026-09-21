#if DEBUG
    import BaeKit
    import Foundation

    /// Preview fixtures for what a candidate row states about its release:
    /// whether its draft was read from a catalog record, and the row still
    /// being asked which pressing it is.
    extension PreviewData {
        private static func glyphCandidate(_ name: String) -> Candidate {
            importTabFolder(
                path: name,
                name: name,
                identifyState: searchStateNotFound.identifyState
            )
        }

        private static let glyphSummary = BridgeTriageMetadataSummary(
            albumTitle: "Album Title Fifteen",
            albumArtistAssignments: [newArtist("Artist Name")]
        )

        /// A settled row whose draft was read from a record, or was not.
        private static func glyphRow(
            _ folder: String,
            readFromRecord: Bool,
            placement: BridgeTriagePlacement = .ready
        ) -> BridgeTriageRow {
            triageRow(
                for: glyphCandidate(folder),
                placement: placement,
                skipAction: .skip,
                actions: [
                    .importReady, .identify, .resetToTags, .clearMetadata,
                    .skip,
                ],
                matched: nil,
                metadataSummary: glyphSummary,
                coverThumbnail: .local(path: previewArtPath("Front.png")),
                metadataProvenance: readFromRecord
                    ? .externalRelease(
                        record: BridgeMetadataRef(
                            catalog: .musicBrainz,
                            key: "rel-paired"
                        ),
                        partners: []
                    )
                    : nil,
                reading: readFromRecord
                    ? .identified(records: identifiedFromBothCatalogs)
                    : .prefilled,
                verification: readFromRecord ? releaseVerification : nil,
                verified: readFromRecord
            )
        }

        /// The draft was read from a catalog record.
        static let triageRowReadFromRecord = glyphRow(
            "Release Folder Fifteen",
            readFromRecord: true
        )

        /// The draft came off the files' own tags rather than any record.
        static let triageRowNotReadFromRecord = glyphRow(
            "Release Folder Sixteen",
            readFromRecord: false
        )

        /// Several pressings are still in question, so no record is chosen
        /// and the row draws the question's chip instead.
        static let triageRowSeveralMatches = glyphRow(
            "Release Folder Nineteen",
            readFromRecord: false,
            placement: .needsYou(reason: .severalMatches(count: 3))
        )
    }
#endif
