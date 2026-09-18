#if DEBUG
    import BaeKit
    import Foundation

    /// Preview fixtures for the two facts a candidate row states about its
    /// release: the four combinations of the seal and the check, and the row
    /// still being asked which pressing it is.
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

        /// A settled row, with whichever of the two facts it states. Each
        /// pairing is its own row so the four combinations, and the question
        /// a row can still be carrying, all render side by side.
        private static func glyphRow(
            _ folder: String,
            identifiedBy: BridgeMarkKind?,
            verified: Bool,
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
                metadataProvenance: identifiedBy == nil
                    ? nil
                    : .externalRelease(
                        record: BridgeMetadataRef(
                            catalog: .musicBrainz,
                            key: "rel-paired"
                        ),
                        partners: []
                    ),
                reading: identifiedBy == nil
                    ? .prefilled
                    : .identified(records: identifiedFromBothCatalogs),
                marks: releaseMarks,
                verification: verified ? releaseVerification : nil,
                identifiedBy: identifiedBy,
                verified: verified
            )
        }

        /// A disc ID tied the files to the record, and the rip databases
        /// found other copies of the disc: both glyphs.
        static let triageRowSealAndCheck = glyphRow(
            "Release Folder Fifteen",
            identifiedBy: .discId,
            verified: true
        )

        /// A barcode tied the files to the record, and no database confirmed
        /// the bits: the seal alone.
        static let triageRowSealOnly = glyphRow(
            "Release Folder Sixteen",
            identifiedBy: .barcode,
            verified: false
        )

        /// The bits matched other copies, and the draft came off the files'
        /// own tags rather than any record: the check alone.
        static let triageRowCheckOnly = glyphRow(
            "Release Folder Seventeen",
            identifiedBy: nil,
            verified: true
        )

        /// Neither fact holds, and the row draws no glyph at all.
        static let triageRowNeitherGlyph = glyphRow(
            "Release Folder Eighteen",
            identifiedBy: nil,
            verified: false
        )

        /// Several pressings are still in question, so no record is chosen
        /// and there is no seal — but the bits matched other copies either
        /// way, and the check sits beside the question's chip.
        static let triageRowCheckBesideMatches = glyphRow(
            "Release Folder Nineteen",
            identifiedBy: nil,
            verified: true,
            placement: .needsYou(reason: .severalMatches(count: 3))
        )
    }
#endif
