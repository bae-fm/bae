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

        /// The folder's images, as the gallery shows them.
        static let mappingImages: [BridgeMappingImage] = [
            mappingImage(coverImage),
            mappingImage(backImage),
            mappingImage(scanImage),
        ]

        private static func mappingImage(
            _ file: BridgeCandidateFile,
        ) -> BridgeMappingImage {
            BridgeMappingImage(
                fileId: file.file.name,
                name: file.file.fileName,
                size: file.file.size,
                localPath: file.file.localPath,
            )
        }

        /// The mapping the picked release produces: nine files against nine
        /// tracks, every row paired, with the folder's images and documents
        /// carried alongside them.
        static let mappingTable = BridgeMappingTable(
            images: mappingImages,
            rows: (1...9).map(mappingTrackRow) + [
                carriedRow(
                    infoLog,
                    role: .document,
                    becomes: .kept
                )
            ],
            reconciliation: .agrees(count: 9)
        )

        private static let moreTracksAudio = BridgeCandidateFile(
            file: BridgeFileInfo(
                name: "Album Image.flac",
                size: 221_100_000,
                dirPrefix: nil,
                fileName: "Album Image.flac",
                localPath: "/tmp/fake/Album Image.flac"
            ),
            role: .audio,
            becomes: .slots(first: 1, last: 1),
            alternatives: [.audio, .notATrack],
            roleChoice: .audio
        )

        static let moreTracksCandidateFiles = BridgeCandidateFiles(
            files: [moreTracksAudio],
            formatLabel: "FLAC",
            collapsedDirectories: []
        )

        static let moreTracksEditValues: BridgeRawReleaseEdit = {
            var edit = editMetadataDraft(trackCount: 10)
            edit.tracks = edit.tracks.enumerated()
                .map { index, track in
                    var track = track
                    track.file =
                        index == 0
                        ? .standalone(fileId: moreTracksAudio.file.name) : nil
                    return track
                }
            return edit
        }()

        private static let moreTracksReleaseDetail = BridgeReleaseDetail(
            releaseId: "rel-more-tracks",
            source: .musicBrainz,
            sourceGroupId: "rg-more-tracks",
            title: moreTracksEditValues.albumTitle,
            artist: "Artist Name",
            year: 1997,
            format: "CD",
            label: "Some Label",
            catalogNumber: "CAT-0001",
            country: "US",
            barcode: "000000000000",
            trackCount: 10,
            tracks: moreTracksEditValues.tracks.enumerated()
                .map {
                    index,
                    track in
                    BridgeReleaseTrack(
                        title: track.title,
                        artist: nil,
                        durationMs: UInt64(252_000 + index * 9000),
                        position: "\(index + 1)",
                        side: 1
                    )
                },
            coverArt: [],
            defaultCover: nil
        )

        /// One file paired to the release's first track, followed by every
        /// release track the folder has nothing for.
        static let moreTracksMappingTable = BridgeMappingTable(
            images: [],
            rows: moreTracksEditValues.tracks.enumerated()
                .map {
                    index,
                    track in
                    let sourcePosition = "\(index + 1)"
                    let sourceDurationMs = UInt64(252_000 + index * 9000)
                    if index == 0 {
                        return .unit(
                            unit: BridgeMappingUnit(
                                source: .file(
                                    file: BridgeMappingFile(
                                        fileId: moreTracksAudio.file.name,
                                        name: moreTracksAudio.file.fileName,
                                        size: moreTracksAudio.file.size,
                                        localPath: moreTracksAudio.file
                                            .localPath,
                                        probedDurationMs: 272_000,
                                        role: .audio,
                                        alternatives: moreTracksAudio
                                            .alternatives,
                                        roleChoice: moreTracksAudio.roleChoice
                                    )
                                ),
                                becomes: .track(
                                    track: track,
                                    sourcePosition: sourcePosition,
                                    sourceDurationMs: sourceDurationMs
                                )
                            )
                        )
                    }
                    return .unit(
                        unit: BridgeMappingUnit(
                            source: .missing,
                            becomes: .track(
                                track: track,
                                sourcePosition: sourcePosition,
                                sourceDurationMs: sourceDurationMs
                            )
                        )
                    )
                },
            reconciliation: .moreTracks(files: 1, tracks: 10)
        )

        /// The same folder while its metadata draft is blank: what each file
        /// is, with what its audio becomes left open.
        static let awaitingPickTable = BridgeMappingTable(
            images: mappingImages,
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
                    carriedRow(
                        infoLog,
                        role: .document,
                        becomes: .kept
                    )
                ],
            reconciliation: nil
        )

        /// One entry the folder's sheet carves out of its single container.
        private static func sheetEntryUnit(
            _ index: Int
        ) -> BridgeMappingUnit {
            let durationMs = UInt64(180_000 + (index + 1) * 15000)
            let entry = BridgeMappingEntry(
                sheetId: boundTrackSheet.file.name,
                index: UInt32(index),
                number: UInt32(index + 1),
                title: "Track Title \(index + 1)",
                durationMs: durationMs,
                containerId: mappedAudioContainer.file.name,
                containerName: mappedAudioContainer.file.fileName,
                containerLocalPath: mappedAudioContainer.file.localPath
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
            sheetId: boundTrackSheet.file.name,
            name: boundTrackSheet.file.fileName,
            localPath: boundTrackSheet.file.localPath,
            bound: .describes(
                container: BridgeMappingContainer(
                    fileId: mappedAudioContainer.file.name,
                    name: mappedAudioContainer.file.fileName,
                    size: mappedAudioContainer.file.size
                )
            ),
            assignment: .disc(number: 1),
            discOptions: [1, 2]
        )

        /// One CUE+FLAC container the sheet carves nine entries out of, with
        /// the folder's images and a collapsed logs directory alongside it.
        static let sheetMappingTable = BridgeMappingTable(
            images: mappingImages,
            rows: [
                .sheet(
                    sheet: previewSheetGroup,
                    entries: (0..<9).map(sheetEntryUnit)
                ),
                .directory(directory: previewLogsDirectory),
            ],
            reconciliation: .agrees(count: 9)
        )

        /// Every row kind the table draws, with the folder's gallery beside it:
        /// the sheet heading the nine entries it carves, the two documents
        /// carried with the release, the diagnostic logs collapsed to the one
        /// row their shared role makes of them, and one non-audio video. What
        /// the Import-tab preview reads, so a change to any row kind shows up
        /// in the canvas without hunting for the fixture that has it.
        static let everyRowKindMappingTable = BridgeMappingTable(
            images: mappingImages,
            rows: [
                .sheet(
                    sheet: previewSheetGroup,
                    entries: (0..<9).map(sheetEntryUnit)
                ),
                carriedRow(infoLog, role: .document, becomes: .kept),
                .directory(directory: previewLogsDirectory),
                carriedRow(notesDocument, role: .document, becomes: .kept),
                carriedRow(
                    supplementalVideo,
                    role: .other,
                    becomes: .kept
                ),
            ],
            reconciliation: .agrees(count: 9)
        )

        /// What the folder's own tags say it is: nine tracks, and no release to
        /// tally them against.
        static let fileTagsMappingTable = BridgeMappingTable(
            images: [],
            rows: (1...9)
                .map { index in
                    BridgeMappingRow.unit(
                        unit: BridgeMappingUnit(
                            source: .file(file: mappingAudio(index)),
                            becomes: .track(
                                track: BridgeRawTrackEdit(
                                    id: "file-tags-track-\(index - 1)",
                                    title: "Track Title \(index)",
                                    artistAssignments: .albumArtists,
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
        ) -> BridgeFolderCandidate {
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/\(name)",
                sourceFolderName: name,
                watchedFolderPath: importWatchedFolder.path,
                files: files,
                trackCount: 9,
                skipped: false,
                isAdded: false
            )
        }

        /// A candidate as the per-candidate read answers for it: the folder,
        /// the row the list places it as, and everything the pane draws. The
        /// pane holds none of it — this is the one value it renders from.
        @MainActor
        static func paneCandidate(
            folder: BridgeFolderCandidate,
            metadataProvenance: BridgeMetadataProvenance? = nil,
            initialMetadataSource: BridgeDefaultImportMetadataSource = .none,
            release: BridgeReleaseDetail? = nil,
            edit: BridgeRawReleaseEdit = blankDraftValues,
            mapping: BridgeMappingTable,
            cover: BridgeCoverChoice? = nil,
            failure: BridgeImportFailure? = nil,
            unprobed: [BridgeAudioFile] = []
        ) -> Candidate {
            Candidate(
                detail: BridgeImportCandidateDetail(
                    candidate: folder,
                    actionable: true,
                    resumedIdentifyState: .idle,
                    row: BridgeTriageRow(
                        candidateKey: folder.folderPath,
                        folderName: folder.sourceFolderName,
                        watchedFolderPath: folder.watchedFolderPath,
                        displayPath: folder.sourceFolderName,
                        resolvedBoundaries: [],
                        combineAncestorKey: nil,
                        actionable: true,
                        placement: metadataProvenance == nil
                            && edit.albumTitle.isEmpty ? .pending : .ready,
                        skipAction: .skip,
                        matched: nil,
                        metadataSummary: nil,
                        coverThumbnail: cover?.thumbnailSource,
                        selectable: !edit.albumTitle.isEmpty,
                        importStatus: nil,
                        metadataProvenance: metadataProvenance
                    ),
                    release: release,
                    pickedLibraryStatus: nil,
                    fileEvidence: [],
                    metadataDraft: edit,
                    metadataDraftIsBlank: edit.albumTitle.isEmpty,
                    metadataProvenance: metadataProvenance,
                    metadataRevision: 1,
                    initialMetadataSource: initialMetadataSource,
                    mapping: mapping,
                    unprobed: unprobed,
                    cover: cover,
                    signals: nil,
                    failure: failure
                )
            )
        }

        /// A candidate with a release selected: the metadata card states what
        /// it is, the mapping table pairs nine files with nine tracks,
        /// and the commit bar counts them.
        @MainActor
        static let mappingCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: candidateFilesTracks
            ),
            metadataProvenance: .externalRelease(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId
            ),
            release: releaseDetailBridge,
            edit: confirmEditValues,
            mapping: mappingTable,
            cover: releaseDetailBridge.defaultCover,
        )

        /// Nothing selected yet: the metadata card offers to find the release,
        /// the table says what each file is with its BECOMES half open, and
        /// there is nothing to commit.
        @MainActor
        static let unidentifiedMappingCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: bridgeCandidateFiles
            ),
            mapping: blankDraftMappingTable,
        )

        /// The same unresolved folder immediately after File Tags is opened,
        /// before its lazy read begins.
        @MainActor
        static let unreadFileTagsMappingCandidate: Candidate = {
            var candidate = unidentifiedMappingCandidate
            candidate.metadataPresentation = .fileTags
            return candidate
        }()

        /// The same unresolved folder after its tags have been read, before
        /// they are applied to its metadata draft.
        @MainActor
        static let unidentifiedFileTagsMappingCandidate: Candidate = {
            var candidate = unidentifiedMappingCandidate
            candidate.metadataPresentation = .fileTags
            candidate.fileTagsPreview = .loaded(releaseSeedBridge)
            return candidate
        }()

        @MainActor
        static let loadingFileTagsMappingCandidate: Candidate = {
            var candidate = unidentifiedMappingCandidate
            candidate.metadataPresentation = .fileTags
            candidate.fileTagsPreview = .loading(
                CandidateFileTagsPreviewSession()
            )
            return candidate
        }()

        @MainActor
        static let blankDraftMappingCandidate = unidentifiedMappingCandidate

        @MainActor
        static let directEntryMappingCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: candidateFilesTracks
            ),
            edit: confirmEditValues,
            mapping: mappingTable
        )

        /// The same unresolved folder with identification settled on several
        /// pressings — the metadata section offers them inline.
        @MainActor
        static let severalMatchesMappingCandidate: Candidate = {
            var candidate = unidentifiedMappingCandidate
            candidate.metadataPresentation = .findOnline
            candidate.resumedIdentifyState = .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 12,
                provenance: searchProvenanceExact
            )
            return candidate
        }()

        /// The CUE+FLAC shape of the same release: one group row over the
        /// entries its sheet carves.
        @MainActor
        static let sheetMappingCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: candidateFilesTracks
            ),
            metadataProvenance: .externalRelease(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId
            ),
            release: releaseDetailBridge,
            edit: confirmEditValues,
            mapping: sheetMappingTable,
            cover: releaseDetailBridge.defaultCover,
        )

        /// A settled ten-track release against a folder containing one audio
        /// file: the first row is backed and the remaining nine are missing.
        @MainActor
        static let moreTracksMappingCandidate: Candidate = paneCandidate(
            folder: BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Partial Album",
                sourceFolderName: "Partial Album",
                watchedFolderPath: importWatchedFolder.path,
                files: moreTracksCandidateFiles,
                trackCount: 1,
                skipped: false,
                isAdded: false
            ),
            metadataProvenance: .externalRelease(
                source: moreTracksReleaseDetail.source,
                releaseId: moreTracksReleaseDetail.releaseId
            ),
            release: moreTracksReleaseDetail,
            edit: moreTracksEditValues,
            mapping: moreTracksMappingTable,
            cover: moreTracksReleaseDetail.defaultCover,
        )

        /// The candidate the Import-tab preview selects: the settled release
        /// read off the CUE+FLAC folder it came from, with every row kind in
        /// its table. Keyed to `folderCandidates`' first, so the row the
        /// sidebar selects and the pane behind it are the same folder.
        @MainActor
        static let importTabCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: bridgeCandidateFiles
            ),
            metadataProvenance: .externalRelease(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId
            ),
            release: releaseDetailBridge,
            edit: confirmEditValues,
            mapping: everyRowKindMappingTable,
            cover: releaseDetailBridge.defaultCover,
        )

        /// The folder read as its own file tags: no release, and a table with
        /// no tally to state.
        @MainActor
        static let fileTagsMappingCandidate: Candidate = paneCandidate(
            folder: mappingFolder(
                name: "Album Title One",
                files: candidateFilesTracks
            ),
            metadataProvenance: .fileTags,
            edit: confirmEditValues,
            mapping: fileTagsMappingTable,
        )

        /// Direct entry starts with blank editable metadata while retaining the
        /// candidate's physical audio-to-track mapping.
        private static let blankDraftValues = BridgeRawReleaseEdit(
            albumTitle: "",
            albumArtistAssignments: [],
            pressing: BridgeRawPressingEdit(
                year: "",
                format: "",
                label: "",
                catalogNumber: "",
                country: "",
                barcode: ""
            ),
            tracks: (1...9)
                .map { index in
                    BridgeRawTrackEdit(
                        id: "draft-track-\(index - 1)",
                        title: "",
                        artistAssignments: .albumArtists,
                        side: 1,
                        trackNumber: Int32(index),
                        file: .standalone(fileId: "Track \(index).flac")
                    )
                }
        )

        private static let blankDraftMappingTable = BridgeMappingTable(
            images: mappingImages,
            rows: blankDraftValues.tracks.enumerated()
                .map { index, track in
                    BridgeMappingRow.unit(
                        unit: BridgeMappingUnit(
                            source: .file(file: mappingAudio(index + 1)),
                            becomes: .track(
                                track: track,
                                sourcePosition: nil,
                                sourceDurationMs: nil
                            )
                        )
                    )
                },
            reconciliation: nil
        )

    }
#endif
