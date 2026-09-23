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
            attention: BridgeNeedsYou? = nil
        ) -> BridgeTriageRow {
            triageRow(
                for: glyphCandidate(folder),
                placement: .ready,
                attention: attention,
                skipAction: .skip,
                actions: [
                    .importReady, .identify, .resetToFileMetadata,
                    .clearMetadata,
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

        /// The draft came off the files' tags, and a run found several
        /// pressings the person has not looked at yet: the row is ready and
        /// flags what the run found.
        static let triageRowSeveralMatches = glyphRow(
            "Release Folder Nineteen",
            readFromRecord: false,
            attention: .severalMatches(count: 3)
        )
    }
#endif
