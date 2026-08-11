#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the import mapping table and its candidate states.
    extension PreviewData {
        // MARK: - The mapping table

        /// One of the folder's audio files, as the mapping table's left half
        /// shows it. Track 4's file runs long against the release, which is
        /// what the row's two lengths are there to show.
        private static func mappingAudio(_ index: Int) -> BridgeMappingFile {
            let drift: Int = index == 4 ? 21000 : 0
            return BridgeMappingFile(
                fileId: "Track \(index).flac",
                name: "Track \(index).flac",
                size: UInt64(35_000_000 + index * 2_000_000),
                localPath: "/tmp/fake/Track \(index).flac",
                probedDurationMs: UInt64(180_000 + index * 15000 + drift),
                role: .audio,
                alternatives: [.audio, .notATrack],
                roleChoice: .audio
            )
        }

        private static func mappingFile(
            _ file: BridgeCandidateFile,
            role: BridgeMappingRole
        ) -> BridgeMappingFile {
            BridgeMappingFile(
                fileId: file.file.name,
                name: file.file.fileName,
                size: file.file.size,
                localPath: file.file.localPath,
                probedDurationMs: nil,
                role: role,
                alternatives: file.alternatives,
                roleChoice: file.roleChoice
            )
        }

        /// One audio file and the track the release puts on it.
        private static func mappingTrackRow(_ index: Int) -> BridgeMappingRow {
            .unit(
                unit: BridgeMappingUnit(
                    source: .file(file: mappingAudio(index)),
                    becomes: .track(
                        track: confirmEditValues.tracks[index - 1],
                        sourcePosition: "\(index)",
                        sourceDurationMs: UInt64(180_000 + index * 15000)
                    )
                )
            )
        }

        /// One of the folder's files that is not one of the release's tracks.
        private static func carriedRow(
            _ file: BridgeCandidateFile,
            role: BridgeMappingRole,
            becomes: BridgeMappingBecomes
        ) -> BridgeMappingRow {
            .unit(
                unit: BridgeMappingUnit(
                    source: .file(file: mappingFile(file, role: role)),
                    becomes: becomes
                )
            )
        }

        /// The folder's images, as the gallery row shows them.
        static let mappingImages: [BridgeMappingImage] = [
            BridgeMappingImage(
                fileId: "Front.png",
                name: "Front.png",
                size: 2_500_000,
                localPath: previewArtPath("Front.png"),
                isCover: true
            ),
            BridgeMappingImage(
                fileId: "Back.png",
                name: "Back.png",
                size: 1_800_000,
                localPath: previewArtPath("Back.png"),
                isCover: false
            ),
            BridgeMappingImage(
                fileId: "scans/scan-1.jpg",
                name: "scan-1.jpg",
                size: 1_400_000,
                localPath: previewArtPath("scan-1.jpg"),
                isCover: false
            ),
        ]

        /// The mapping the picked release produces: nine files against nine
        /// tracks, every row paired, with the folder's images and documents
        /// carried alongside them.
        static let mappingTable = BridgeMappingTable(
            rows: (1...9).map(mappingTrackRow) + [
                .images(images: mappingImages),
                carriedRow(
                    infoLog,
                    role: .document,
                    becomes: .kept
                ),
            ],
            reconciliation: .agrees(count: 9)
        )

        /// The same folder before a release is picked: what each file is, with
        /// what its audio becomes left open.
        static let awaitingPickTable = BridgeMappingTable(
            rows: (1...9)
                .map { index in
                    BridgeMappingRow.unit(
                        unit: BridgeMappingUnit(
                            source: .file(file: mappingAudio(index)),
                            becomes: .awaitingPick
                        )
                    )
                }
                + [
                    .images(images: mappingImages),
                    carriedRow(
                        infoLog,
                        role: .document,
                        becomes: .kept
                    ),
                ],
            reconciliation: nil
        )

        /// One entry the folder's sheet carves out of its single container.
        private static func sheetEntryUnit(
            _ index: Int
        ) -> BridgeMappingUnit {
            let durationMs = UInt64(180_000 + (index + 1) * 15000)
            let entry = BridgeMappingEntry(
                sheetId: "Album Title.cue",
                index: UInt32(index),
                number: UInt32(index + 1),
                title: "Track Title \(index + 1)",
                durationMs: durationMs,
                containerId: "Album Title.flac",
                containerName: "Album Title.flac",
                containerLocalPath: "/tmp/fake/Album Title.flac"
            )
            return BridgeMappingUnit(
                source: .sheetEntry(entry: entry),
                becomes: .track(
                    track: confirmEditValues.tracks[index],
                    sourcePosition: "\(index + 1)",
                    sourceDurationMs: durationMs
                )
            )
        }

        private static let previewSheetGroup = BridgeSheetGroup(
            sheetId: "Album Title.cue",
            name: "Album Title.cue",
            localPath: "/tmp/fake/Album Title.cue",
            bound: .describes(
                container: BridgeMappingContainer(
                    fileId: "Album Title.flac",
                    name: "Album Title.flac",
                    size: 340_000_000
                )
            ),
            assignment: .disc(number: 1),
            discOptions: [1, 2]
        )

        private static let previewLogsDirectory = BridgeCollapsedDirectory(
            dirPrefix: "logs/",
            kind: .document,
            count: 4,
            totalSize: 4 * 6000
        )

        /// One CUE+FLAC container the sheet carves nine entries out of, with
        /// the folder's images and a collapsed logs directory alongside it.
        static let sheetMappingTable = BridgeMappingTable(
            rows: [
                .sheet(
                    sheet: previewSheetGroup,
                    entries: (0..<9).map(sheetEntryUnit)
                ),
                .images(images: mappingImages),
                .directory(directory: previewLogsDirectory),
            ],
            reconciliation: .agrees(count: 9)
        )

        /// Every row kind the table draws, in one folder: the sheet heading the
        /// nine entries it carves, the images as one gallery, the two documents
        /// carried with the release, and the rip logs collapsed to the one row
        /// their shared role makes of them. What the Import-tab preview reads,
        /// so a change to any row kind shows up in the canvas without hunting
        /// for the fixture that has it.
        static let everyRowKindMappingTable = BridgeMappingTable(
            rows: [
                .sheet(
                    sheet: previewSheetGroup,
                    entries: (0..<9).map(sheetEntryUnit)
                ),
                .images(images: mappingImages),
                carriedRow(infoLog, role: .document, becomes: .kept),
                carriedRow(notesDocument, role: .document, becomes: .kept),
                .directory(directory: previewLogsDirectory),
            ],
            reconciliation: .agrees(count: 9)
        )

        /// What the folder's own tags say it is: nine tracks, and no release to
        /// tally them against.
        static let unknownMappingTable = BridgeMappingTable(
            rows: (1...9)
                .map { index in
                    BridgeMappingRow.unit(
                        unit: BridgeMappingUnit(
                            source: .file(file: mappingAudio(index)),
                            becomes: .track(
                                track: BridgeRawTrackEdit(
                                    id: "unknown-track-\(index - 1)",
                                    title: "Track Title \(index)",
                                    artistText: "",
                                    side: 1,
                                    trackNumber: Int32(index),
                                    file: .standalone(
                                        fileId: "Track \(index).flac"
                                    )
                                ),
                                sourcePosition: "\(index)",
                                sourceDurationMs: nil
                            )
                        )
                    )
                },
            reconciliation: nil
        )

        // MARK: - Mapping pane candidates

        private static func mappingFolder(
            name: String,
            files: BridgeCandidateFiles
        ) -> Candidate {
            Candidate(
                bridge: BridgeFolderCandidate(
                    folderPath: "/Music/Downloads/\(name)",
                    sourceFolderName: name,
                    watchedFolderPath: importWatchedFolder.path,
                    files: files,
                    trackCount: 9,
                    skipped: false,
                    isAdded: false
                )
            )
        }

        /// A candidate with a release picked: the identity card states the
        /// claim, the mapping table pairs nine files with nine tracks, and the
        /// commit bar counts them.
        static let mappingCandidate: Candidate = {
            var candidate = mappingFolder(
                name: "Album Title One",
                files: candidateFilesTracks
            )
            candidate.claim = claimBridge
            candidate.identityChoice = claimBridge.choice
            candidate.pick = CandidatePick(
                releaseId: releaseDetailBridge.releaseId,
                source: releaseDetailBridge.source,
                claim: claimBridge.level
            )
            candidate.releaseDetailBridge = releaseDetailBridge
            candidate.editValues = confirmEditValues
            candidate.mapping = mappingTable
            return candidate
        }()

        /// Nothing picked yet: the identity card offers to find the release,
        /// the table says what each file is with its BECOMES half open, and
        /// there is nothing to commit.
        static let unidentifiedMappingCandidate: Candidate = {
            var candidate = mappingFolder(
                name: "Album Title One",
                files: bridgeCandidateFiles
            )
            candidate.mapping = awaitingPickTable
            return candidate
        }()

        /// The same unpicked folder with identification settled on several
        /// pressings — the identity section offers them inline.
        static let severalMatchesMappingCandidate: Candidate = {
            var candidate = unidentifiedMappingCandidate
            candidate.identifyState = .found(
                group: searchGroupExact,
                libraryStatuses: [:],
                trackCount: 12,
                provenance: searchProvenanceExact
            )
            return candidate
        }()

        /// The CUE+FLAC shape of the same release: one group row over the
        /// entries its sheet carves.
        static let sheetMappingCandidate: Candidate = {
            var candidate = mappingCandidate
            candidate.mapping = sheetMappingTable
            return candidate
        }()

        /// The candidate the Import-tab preview selects: the settled release
        /// read off the CUE+FLAC folder it came from, with every row kind in
        /// its table. Keyed to `folderCandidates`' first, so the row the
        /// sidebar selects and the pane behind it are the same folder.
        static let importTabCandidate: Candidate = {
            var candidate = mappingCandidate
            candidate.files = bridgeCandidateFiles
            candidate.mapping = everyRowKindMappingTable
            return candidate
        }()

        /// The folder read as its own file tags: no claim, no release detail,
        /// and a table with no tally to state.
        static let unknownMappingCandidate: Candidate = {
            var candidate = mappingCandidate
            candidate.identity = .unknown
            candidate.identityChoice = .unknown
            candidate.claim = nil
            candidate.releaseDetailBridge = nil
            candidate.mapping = unknownMappingTable
            return candidate
        }()

    }
#endif
