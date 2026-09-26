#if DEBUG
    import BaeKit
    import Foundation

    /// Preview fixtures for what a candidate row states about its release:
    /// whether its draft was read from a catalog record.
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
            albumArtistAssignments: [artistCredit("Artist Name")]
        )

        /// A settled row whose draft was read from a record, or was not.
        private static func glyphRow(
            _ folder: String,
            readFromRecord: Bool
        ) -> BridgeTriageRow {
            triageRow(
                for: glyphCandidate(folder),
                placement: .ready,
                selectable: true,
                matched: nil,
                metadataSummary: glyphSummary,
                cover: .local(path: previewArtPath("Front.png")),
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
    }
#endif
