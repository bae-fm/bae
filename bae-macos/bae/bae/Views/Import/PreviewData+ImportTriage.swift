#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the import triage sidebar, candidate file listings,
    /// and the release chosen for a candidate.
    extension PreviewData {
        private static let previewSourceAudioFormat = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )

        // MARK: - Triage sidebar

        /// A `BridgeMatchedRelease` fixture with a settled single pressing —
        /// the shape a Ready or Done row's `matched` carries.
        static func triageMatch(
            releaseId: String,
            title: String,
            artist: String? = "Artist Name",
            year: Int32? = 1997,
            format: String? = "CD",
            trackCount: UInt32 = 12,
            source: BridgeMetadataSource = .musicBrainz,
            signal: BridgeMatchedSignal? = .discId
        ) -> BridgeMatchedRelease {
            BridgeMatchedRelease(
                releaseId: releaseId,
                title: title,
                artist: artist,
                pressing: BridgeMatchedPressing(
                    year: year,
                    format: format,
                    trackCount: trackCount
                ),
                // In fixtures the "URL" is a path to generated placeholder
                // art; `PreviewData.artImageStore()` serves it from disk.
                coverThumbnailUrl: previewArtPath(title),
                evidence: BridgeMatchEvidence(source: source, signal: signal)
            )
        }

        /// A `BridgeTriageRow` fixture keyed to an existing `Candidate` fixture,
        /// so the sidebar and the detail pane agree on the same folder.
        static func triageRow(
            for candidate: Candidate,
            placement: BridgeTriagePlacement,
            skipAction: BridgeTriageSkipAction?,
            matched: BridgeMatchedRelease?,
            metadataSummary: BridgeTriageMetadataSummary? = nil,
            coverThumbnail: BridgeCoverImageSource? = nil,
            selectable: Bool,
            importStatus: BridgeTriageImportStatus? = nil,
            metadataProvenance: BridgeMetadataProvenance? = nil
        ) -> BridgeTriageRow {
            BridgeTriageRow(
                candidateKey: candidate.key,
                folderName: candidate.displayName,
                watchedFolderPath: importWatchedFolder.path,
                displayPath: candidate.displayName,
                resolvedBoundaries: [],
                combineAncestorKey: nil,
                actionable: true,
                placement: placement,
                skipAction: skipAction,
                matched: matched,
                metadataSummary: metadataSummary,
                coverThumbnail: coverThumbnail,
                selectable: selectable,
                importStatus: importStatus,
                metadataProvenance: metadataProvenance
            )
        }

        /// A preview candidate showing `identifyState`. Nothing is running in
        /// a preview, so the state stands as the one its stored verdict would
        /// resume — which is what every surface falls back to.
        private static func importTabFolder(
            path: String,
            name: String,
            trackCount: UInt32 = 9,
            identifyState: IdentifyState
        ) -> Candidate {
            var candidate = Candidate(
                bridge: BridgeFolderCandidate(
                    folderPath: "\(importWatchedFolder.path)/\(path)",
                    sourceFolderName: name,
                    watchedFolderPath: importWatchedFolder.path,
                    files: bridgeCandidateFiles,
                    trackCount: trackCount,
                    skipped: false,
                    isAdded: false
                )
            )
            candidate.resumedIdentifyState = identifyState
            return candidate
        }

        static let importTabSeveralMatchesCandidate = importTabFolder(
            path: "Album Title Five",
            name: "Album Title Five",
            trackCount: 12,
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 12,
                provenance: searchProvenanceExact
            )
        )

        static let importTabDisagreementCandidate = importTabFolder(
            path: "Album Title Six - Remaster",
            name: "Album Title Six - Remaster",
            trackCount: 11,
            identifyState: searchStateDisagreement.identifyState
        )

        private static let trackMismatchGroup = ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "group-track-mismatch",
                sourceGroupId: "group-track-mismatch",
                title: "Album Title Seven",
                artist: "Artist Name",
                coverArt: nil,
                sourceLabel: "MusicBrainz",
                groupUrl:
                    "https://musicbrainz.org/release-group/group-track-mismatch",
                yearMin: 1994,
                yearMax: 1994,
                pressings: [exactPressings[0]]
            )
        )

        static let importTabTrackMismatchCandidate = importTabFolder(
            path: "Album Title Seven - Partial",
            name: "Album Title Seven - Partial",
            trackCount: 1,
            identifyState: .found(
                groups: [trackMismatchGroup],
                libraryStatuses: [:],
                trackCount: 1,
                provenance: [:]
            )
        )

        private static let importTabAlreadyInLibraryStatus =
            BridgeLibraryStatus(
                releaseId: releaseDetailBridge.releaseId,
                releaseInLibrary: true,
                albumInLibrary: true,
                albumTitle: releaseDetailBridge.title,
                albumId: "album-in-library"
            )

        static let importTabAlreadyInLibraryCandidate = importTabFolder(
            path: "Album Title Eight - Reissue",
            name: "Album Title Eight - Reissue",
            trackCount: 14,
            identifyState: .found(
                groups: [searchGroupExact],
                libraryStatuses: [
                    releaseDetailBridge.releaseId:
                        importTabAlreadyInLibraryStatus
                ],
                trackCount: 14,
                provenance: searchProvenanceExact
            )
        )

        static let importTabNoMatchCandidate = importTabFolder(
            path: "Unmatched Folder",
            name: "Unmatched Folder",
            identifyState: searchStateNotFound.identifyState
        )

        static let importTabIdentifyingCandidate = importTabFolder(
            path: "Queued Folder",
            name: "Queued Folder",
            identifyState: searchStateTriangulating.identifyState
        )

        @MainActor
        private static let importTabGroupedReadyCandidate = paneCandidate(
            folder: BridgeFolderCandidate(
                folderPath:
                    "\(importWatchedFolder.path)/Artist Collection/Album Title Nine",
                sourceFolderName: "Album Title Nine",
                watchedFolderPath: importWatchedFolder.path,
                files: bridgeCandidateFiles,
                trackCount: 9,
                skipped: false,
                isAdded: false
            ),
            metadataProvenance: .externalRelease(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId
            ),
            release: releaseDetailBridge,
            edit: confirmEditValues,
            mapping: everyRowKindMappingTable,
            cover: releaseDetailBridge.defaultCover
        )

        @MainActor
        private static let importTabGroupedCandidates = [
            importTabGroupedReadyCandidate,
            importTabFolder(
                path: "Artist Collection/Album Title Ten",
                name: "Album Title Ten",
                identifyState: searchStateNotFound.identifyState
            ),
        ]

        @MainActor
        static let triageRowReady = triageRow(
            for: importTabCandidate,
            placement: .ready,
            skipAction: .skip,
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: releaseDetailBridge.title,
                artist: releaseDetailBridge.artist,
                year: releaseDetailBridge.year,
                format: releaseDetailBridge.format,
                trackCount: releaseDetailBridge.trackCount
            ),
            metadataSummary: nil,
            selectable: true,
            metadataProvenance: .externalRelease(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId
            )
        )

        static let triageRowPickAPressing = triageRow(
            for: importTabSeveralMatchesCandidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 2)
                )
            ),
            skipAction: .skip,
            // Several matches — the pressing is exactly what's unsettled, so
            // there is no `pressing` to show yet, only the lead's title and
            // artist.
            matched: BridgeMatchedRelease(
                releaseId: "rel-lead",
                title: "Album Title Five",
                artist: "Artist Name",
                pressing: nil,
                coverThumbnailUrl: nil,
                evidence: BridgeMatchEvidence(
                    source: .musicBrainz,
                    signal: nil
                )
            ),
            metadataSummary: nil,
            selectable: false,
            importStatus: nil
        )

        /// Two signals that named different releases: the row asks the same
        /// question any multi-match does.
        static let triageRowSeveralMatchesFromSignals = triageRow(
            for: importTabDisagreementCandidate,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(disagreement: .severalMatches(count: 2))
            ),
            skipAction: .skip,
            matched: nil,
            metadataSummary: nil,
            selectable: false
        )

        static let triageRowTrackMismatch = triageRow(
            for: importTabTrackMismatchCandidate,
            placement: .needsYou(
                group: .countsOrLengthsDisagree,
                reason: .disagreement(
                    disagreement: .trackCountDisagrees(local: 1, source: 10)
                )
            ),
            skipAction: .skip,
            matched: triageMatch(
                releaseId: "rel-track-mismatch",
                title: "Album Title Seven",
                year: 1994,
                trackCount: 10
            ),
            metadataSummary: nil,
            selectable: false
        )

        static let triageRowAlreadyInLibrary = triageRow(
            for: importTabAlreadyInLibraryCandidate,
            placement: .needsYou(
                group: .alreadyInLibrary,
                reason: .disagreement(disagreement: .alreadyInLibrary)
            ),
            skipAction: .skip,
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: "Album Title (Reissue)",
                year: 2004,
                trackCount: 14,
                signal: .barcode
            ),
            metadataSummary: nil,
            selectable: false
        )

        static let triageRowNoMatch = triageRow(
            for: importTabNoMatchCandidate,
            placement: .needsYou(
                group: .noMatch,
                reason: .disagreement(disagreement: .noMatch)
            ),
            skipAction: .skip,
            matched: nil,
            metadataSummary: nil,
            selectable: false
        )

        static let triageRowStillIdentifying = triageRow(
            for: importTabIdentifyingCandidate,
            placement: .needsYou(
                group: .stillIdentifying,
                reason: .stillIdentifying(phase: .running)
            ),
            skipAction: .skip,
            matched: nil,
            metadataSummary: nil,
            selectable: false
        )

        private static let importTabImportingCandidate = folderCandidates[2]
        private static let importTabDoneCandidate = folderCandidates[3]
        private static let importTabFailedCandidate = folderCandidates[4]

        /// How far the preview's running import has got — what the row's
        /// progress leaf reads off the candidate-runtime signal.
        static let importTabImportInFlight = BridgeImportInFlight(
            progressPercent: 45,
            step: .running(phase: .measuringLoudness)
        )

        static let triageRowImporting = triageRow(
            for: importTabImportingCandidate,
            placement: .importing,
            skipAction: nil,
            matched: triageMatch(
                releaseId: "rel-importing",
                title: importTabImportingCandidate.displayName,
                trackCount: 15
            ),
            metadataSummary: nil,
            selectable: false,
            importStatus: .importing
        )

        static let triageRowSkipped = triageRow(
            for: folderCandidates[1],
            placement: .skipped,
            skipAction: .unskip,
            matched: nil,
            metadataSummary: nil,
            selectable: false
        )

        static let triageRowDoneImported = triageRow(
            for: importTabDoneCandidate,
            placement: .done,
            skipAction: nil,
            matched: triageMatch(
                releaseId: "preview-release",
                title: importTabDoneCandidate.displayName,
                trackCount: 5
            ),
            metadataSummary: nil,
            selectable: false,
            importStatus: .complete(
                releaseId: "preview-release",
                albumId: "preview-album"
            )
        )

        static let triageRowFailed = triageRow(
            for: importTabFailedCandidate,
            placement: .failed,
            skipAction: nil,
            matched: triageMatch(
                releaseId: "rel-failed",
                title: importTabFailedCandidate.displayName,
                trackCount: 18,
                source: .discogs,
                signal: .barcode
            ),
            metadataSummary: nil,
            selectable: false,
            importStatus: .error(
                error: .Diagnostic(
                    category: .import,
                    detail: "track 7 is truncated"
                )
            )
        )

        @MainActor
        private static let triageGroupedRows = [
            triageRow(
                for: importTabGroupedReadyCandidate,
                placement: .ready,
                skipAction: .skip,
                matched: triageMatch(
                    releaseId: releaseDetailBridge.releaseId,
                    title: releaseDetailBridge.title,
                    artist: releaseDetailBridge.artist,
                    year: releaseDetailBridge.year,
                    format: releaseDetailBridge.format,
                    trackCount: releaseDetailBridge.trackCount
                ),
                metadataSummary: nil,
                selectable: true,
                metadataProvenance: .externalRelease(
                    source: releaseDetailBridge.source,
                    releaseId: releaseDetailBridge.releaseId
                )
            ),
            triageRow(
                for: importTabGroupedCandidates[1],
                placement: .needsYou(
                    group: .noMatch,
                    reason: .disagreement(disagreement: .noMatch)
                ),
                skipAction: .skip,
                matched: nil,
                metadataSummary: nil,
                selectable: false
            ),
        ]

        @MainActor
        static let importTabCandidates =
            [
                importTabCandidate,
                importTabSeveralMatchesCandidate,
                importTabDisagreementCandidate,
                importTabTrackMismatchCandidate,
                importTabAlreadyInLibraryCandidate,
                importTabNoMatchCandidate,
                importTabIdentifyingCandidate,
                importTabImportingCandidate,
                importTabDoneCandidate,
                importTabFailedCandidate,
                folderCandidates[1],
            ] + importTabGroupedCandidates

        private static let importTabGroupKey = BridgeFolderReleaseDecisionKey(
            watchedFolderPath: importWatchedFolder.path,
            relativeFolderPath: "Artist Collection"
        )

        @MainActor
        private static let importTabPendingRows = [
            triageRowReady,
            triageRowPickAPressing,
            triageRowSeveralMatchesFromSignals,
            triageRowTrackMismatch,
            triageRowAlreadyInLibrary,
            triageRowNoMatch,
            triageRowStillIdentifying,
            triageRowImporting,
            triageRowFailed,
        ]

        private static let importTabDoneRows = [
            triageRowDoneImported
        ]

        @MainActor
        static func importTabItems(
            _ tab: BridgeTriageTab
        ) -> [BridgeImportListItem] {
            switch tab {
            case .pending:
                return importTabPendingRows.map(candidateItem)
                    + [
                        groupHeaderItem(
                            key: importTabGroupKey,
                            name: "Artist Collection",
                            entryCount: UInt32(triageGroupedRows.count)
                        )
                    ]
                    + triageGroupedRows.map {
                        candidateItem($0, isGroupMember: true)
                    }
            case .done:
                return importTabDoneRows.map(candidateItem)
            case .skipped:
                return [candidateItem(triageRowSkipped)]
                    + invalidCandidates.map(invalidItem)
            }
        }

        @MainActor
        private static let importTabSummary = importQueueSummary(
            pending: 11,
            done: 1,
            skipped: 1 + UInt32(invalidCandidates.count),
            watchedFolders: [importWatchedFolder],
            groupKeys: [importTabGroupKey],
            ready: readyRows(importTabPendingRows + triageGroupedRows),
            firstUnidentified: BridgeFirstUnidentifiedRowRef(
                candidateKey: triageRowStillIdentifying.candidateKey,
                stableKey:
                    "candidate:\(triageRowStillIdentifying.candidateKey)",
                groupKey: nil,
                visiblePosition: 0
            )
        )

        /// One preview of the whole Import tab: the store the sidebar and the
        /// detail pane read, and the items each tab holds. Every candidate row
        /// resolves to the candidate the detail pane opens, while boundary and
        /// invalid entries exercise the two non-candidate shapes.
        @MainActor
        /// Every row the tab holds, whichever tab it is on, by candidate key.
        /// A selected candidate carries the same row the list does — which is
        /// what the row-driven actions (skip, import) read their eligibility
        /// from, so a fixture without it makes every candidate ineligible.
        private static func importTabRowsByKey() -> [String: BridgeTriageRow] {
            let rows =
                importTabPendingRows + triageGroupedRows + importTabDoneRows
                + [triageRowSkipped]
            return Dictionary(
                rows.map { ($0.candidateKey, $0) },
                uniquingKeysWith: { first, _ in first }
            )
        }

        @MainActor
        static func importTabScene() -> ImportPreviewFixture {
            let store = ImportStore()
            store.applySummary(importTabSummary)
            let rows = importTabRowsByKey()
            for var candidate in importTabCandidates {
                candidate.row = rows[candidate.key]
                store.selectedCandidates[candidate.key] = candidate
            }
            store.queueIdentifyProgress = (identified: 112, total: 130)
            return ImportPreviewFixture(
                store: store,
                itemsByTab: [
                    .pending: importTabItems(.pending),
                    .done: importTabItems(.done),
                    .skipped: importTabItems(.skipped),
                ]
            )
        }

        static func importTabImporter() -> Importer {
            let optionsBySheet = sheetBindingOptions
            let importingKey = importTabImportingCandidate.key
            let inFlight = importTabImportInFlight
            return Importer(
                sheetBindingOptions: { _, sheetFileId in
                    guard let options = optionsBySheet[sheetFileId] else {
                        throw StubError.notImplemented
                    }
                    return options
                },
                candidateRuntime: { key in
                    guard key == importingKey else { return nil }
                    return BridgeCandidateRuntimeSnapshot(
                        identifyState: .idle,
                        signalsToolbar: BridgeSignalsToolbar(signals: []),
                        import: inFlight
                    )
                }
            )
        }

        private static func previewFile(
            name: String,
            size: UInt64,
            role: BridgeFileRole,
            becomes: BridgeFileBecomes = .noSlots,
            dirPrefix: String? = nil,
            localPath: String? = nil
        ) -> BridgeCandidateFile {
            // Only audio is a decision, so only audio carries alternatives —
            // the same rule core applies.
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: dirPrefix.map { $0 + name } ?? name,
                    size: size,
                    dirPrefix: dirPrefix,
                    fileName: name,
                    localPath: localPath ?? "/tmp/fake/\(name)",
                    audioFormat: role.isAudio ? previewSourceAudioFormat : nil
                ),
                role: role,
                becomes: becomes,
                alternatives: role.isAudio ? [.audio, .notATrack] : [],
                roleChoice: role.isAudio ? .audio : nil
            )
        }

        private static func previewImage(
            name: String,
            size: UInt64,
            dirPrefix: String? = nil
        ) -> BridgeCandidateFile {
            let path = previewArtPath(name)
            let choice = BridgeCoverChoice(
                selection: .releaseImage(fileId: name),
                previewSource: .local(path: path),
                thumbnailSource: .local(path: path)
            )
            return previewFile(
                name: name,
                size: size,
                role: .artwork(choice: choice),
                dirPrefix: dirPrefix,
                localPath: path
            )
        }

        /// A sheet bound to `Album Title.flac`, carving the release's nine
        /// slots out of it.
        static let boundTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(
                binding: .describes(fileId: mappedAudioContainer.file.name),
                trackCount: 9
            ),
            becomes: .slots(first: 1, last: 9)
        )

        static let mappedAudioContainer = previewFile(
            name: "Album Title.flac",
            size: 340_000_000,
            role: .audio,
            becomes: .slots(first: 1, last: 9)
        )

        static let backImage = previewImage(
            name: "Back.png",
            size: 1_800_000
        )

        static let coverImage = previewImage(
            name: "Front.png",
            size: 2_500_000
        )

        static let scanImage = previewImage(
            name: "scan-1.jpg",
            size: 1_400_000,
            dirPrefix: "scans/"
        )

        /// What core offers a sheet in this folder: the FLAC it can use, and
        /// the MP3 it can't, refused with its codec named.
        static let sheetBindingOptions: [String: [BridgeSheetBindingOption]] = [
            boundTrackSheet.file.name: [
                BridgeSheetBindingOption(
                    fileId: mappedAudioContainer.file.name,
                    offer: .offered
                ),
                BridgeSheetBindingOption(
                    fileId: "Album Title.mp3",
                    offer: .refusedCodec(codec: "MP3")
                ),
            ]
        ]

        static let infoLog = previewFile(
            name: "info.log",
            size: 6000,
            role: .document
        )

        static let notesDocument = previewFile(
            name: "notes.txt",
            size: 1200,
            role: .document
        )

        static let supplementalVideo = previewFile(
            name: "video.mkv",
            size: 24_000_000,
            role: .other
        )

        static let previewLogDocuments = [
            "checksum.txt",
            "drive.txt",
            "read.txt",
            "verify.txt",
        ]
        .map {
            previewFile(
                name: $0,
                size: 6000,
                role: .document,
                dirPrefix: "logs/"
            )
        }

        static let bridgeCandidateFiles = BridgeCandidateFiles(
            fileTagsIdentity: "cue-backed-preview-audio",
            files: [
                backImage,
                boundTrackSheet,
                mappedAudioContainer,
                coverImage,
                scanImage,
                infoLog,
                notesDocument,
                supplementalVideo,
            ]
                + previewLogDocuments,
            sourceAudio: BridgeCandidateSourceAudio(
                summary: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .cue,
                        format: previewSourceAudioFormat
                    )
                ),
                files: [mappedAudioContainer.file]
            )
        )

        static let releaseDetailBridge: BridgeReleaseDetail = {
            let tracks: [BridgeReleaseTrack] = (1...9)
                .map { i in
                    let ms = UInt64(180_000 + i * 15000)
                    return BridgeReleaseTrack(
                        title: "Track Title \(i)",
                        artist: i == 5 ? "Featured Artist" : nil,
                        durationMs: ms,
                        position: "\(i)",
                        side: 1
                    )
                }
            return BridgeReleaseDetail(
                releaseId: "rel-123",
                source: .musicBrainz,
                sourceGroupId: "rg-123",
                title: "Album Title One",
                artist: "Artist Name",
                year: 1996,
                format: "CD",
                label: "Label Name",
                catalogNumber: "6006-2",
                country: "US",
                barcode: nil,
                trackCount: 9,
                tracks: tracks,
                coverArt: [],
                defaultCover: BridgeCoverChoice(
                    selection: .releaseImage(fileId: "Front.png"),
                    previewSource: .local(path: previewArtPath("Front.png")),
                    thumbnailSource: .local(path: previewArtPath("Front.png"))
                )
            )
        }()

        /// Editor seed for the confirming previews — the raw release edit the
        /// prefetch's seed projects into.
        static let confirmEditValues = editMetadataDraft(trackCount: 9)

        /// The same values shaped through the production boundary for previews
        /// that show metadata before it becomes the candidate's raw draft.
        static let releaseSeedBridge: BridgeReleaseUserEdit = {
            guard
                case .valid(let edit) = shapeReleaseEdit(
                    raw: confirmEditValues
                )
            else {
                preconditionFailure("Preview release metadata must be valid")
            }
            return edit
        }()

        /// Per-track audio candidate (nine FLAC files) plus one cover image, two
        /// documents, and a sheet describing nothing yet — the file-per-track
        /// counterpart to `bridgeCandidateFiles`.
        private static let trackAudioFiles: [BridgeCandidateFile] = (1...9)
            .map { (i: Int) -> BridgeCandidateFile in
                let slot = UInt32(i)
                return previewFile(
                    name: "Track \(i).flac",
                    size: UInt64(35_000_000 + i * 2_000_000),
                    role: .audio,
                    becomes: .slots(first: slot, last: slot)
                )
            }

        static let candidateFilesTracks = BridgeCandidateFiles(
            fileTagsIdentity: "file-backed-preview-audio",
            files: trackAudioFiles
                + [
                    coverImage,
                    infoLog,
                    notesDocument,
                    previewFile(
                        name: "Album.cue",
                        size: 1100,
                        role: .trackSheet(
                            binding: .unresolved(requested: ["Album Title.wav"]
                            ),
                            trackCount: 9
                        )
                    ),
                ],
            sourceAudio: BridgeCandidateSourceAudio(
                summary: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: previewSourceAudioFormat
                    )
                ),
                files: trackAudioFiles.map(\.file)
            )
        )

    }
#endif
