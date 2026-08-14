#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the import triage sidebar, candidate file listings,
    /// and the release chosen for a candidate.
    extension PreviewData {
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
            selectable: Bool,
            importStatus: BridgeCandidateImportStatus? = nil,
            picked: BridgeIdentityPick? = nil,
            claim: BridgeIdentityChoice? = nil
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
                selectable: selectable,
                importStatus: importStatus,
                picked: picked,
                claim: claim
            )
        }

        private static func importTabFolder(
            path: String,
            name: String,
            trackCount: UInt32 = 9,
            identifyState: IdentifyState,
            signalsToolbar: BridgeSignalsToolbar
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
            candidate.identifyState = identifyState
            candidate.signalsToolbar = signalsToolbar
            candidate.mapping = awaitingPickTable
            return candidate
        }

        static let importTabSeveralMatchesCandidate = importTabFolder(
            path: "Album Title Five",
            name: "Album Title Five",
            trackCount: 12,
            identifyState: .found(
                group: searchGroupExact,
                libraryStatuses: [:],
                trackCount: 12,
                provenance: searchProvenanceExact
            ),
            signalsToolbar: searchStateFoundExact.signalsToolbar
        )

        static let importTabConflictCandidate = importTabFolder(
            path: "Album Title Six - Remaster",
            name: "Album Title Six - Remaster",
            trackCount: 11,
            identifyState: searchStateConflict.identifyState,
            signalsToolbar: searchStateConflict.signalsToolbar
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
                group: trackMismatchGroup,
                libraryStatuses: [:],
                trackCount: 1,
                provenance: [:]
            ),
            signalsToolbar: searchStateFoundExact.signalsToolbar
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
                group: searchGroupExact,
                libraryStatuses: [
                    releaseDetailBridge.releaseId:
                        importTabAlreadyInLibraryStatus
                ],
                trackCount: 14,
                provenance: searchProvenanceExact
            ),
            signalsToolbar: searchStateFoundExact.signalsToolbar
        )

        static let importTabNoMatchCandidate = importTabFolder(
            path: "Unmatched Folder",
            name: "Unmatched Folder",
            identifyState: searchStateNotFound.identifyState,
            signalsToolbar: searchStateNotFound.signalsToolbar
        )

        static let importTabIdentifyingCandidate = importTabFolder(
            path: "Queued Folder",
            name: "Queued Folder",
            identifyState: searchStateTriangulating.identifyState,
            signalsToolbar: searchStateTriangulating.signalsToolbar
        )

        private static let importTabGroupedCandidates = [
            importTabFolder(
                path: "Artist Collection/Album Title Nine",
                name: "Album Title Nine",
                identifyState: searchStateNotFound.identifyState,
                signalsToolbar: searchStateNotFound.signalsToolbar
            ),
            importTabFolder(
                path: "Artist Collection/Album Title Ten",
                name: "Album Title Ten",
                identifyState: searchStateNotFound.identifyState,
                signalsToolbar: searchStateNotFound.signalsToolbar
            ),
        ]

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
            selectable: true,
            picked: .release(
                source: releaseDetailBridge.source,
                releaseId: releaseDetailBridge.releaseId,
                claim: .exact
            ),
            claim: claimBridge.choice
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
            selectable: false,
            importStatus: nil
        )

        static let triageRowSignalsConflict = triageRow(
            for: importTabConflictCandidate,
            placement: .needsYou(
                group: .signalsDisagree,
                reason: .disagreement(disagreement: .signalsConflict)
            ),
            skipAction: .skip,
            matched: nil,
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
            selectable: false
        )

        private static func candidate(
            _ candidate: Candidate,
            withImportStatus importStatus: BridgeCandidateImportStatus
        ) -> Candidate {
            var candidate = candidate
            candidate.importStatus = importStatus
            return candidate
        }

        private static let importTabImportingCandidate = candidate(
            folderCandidates[2],
            withImportStatus: .importing(
                progressPercent: 45,
                step: .running(phase: .measuringLoudness)
            )
        )

        private static let importTabDoneCandidate = candidate(
            folderCandidates[3],
            withImportStatus: .complete(
                releaseId: "preview-release",
                albumId: "preview-album"
            )
        )

        private static let importTabFailedCandidate = candidate(
            folderCandidates[4],
            withImportStatus: .error(
                error: .Diagnostic(
                    category: .import,
                    detail: "track 7 is truncated"
                )
            )
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
            selectable: false,
            importStatus: importTabImportingCandidate.importStatus
        )

        static let triageRowSkipped = triageRow(
            for: folderCandidates[1],
            placement: .skipped,
            skipAction: .unskip,
            matched: nil,
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
            selectable: false,
            importStatus: importTabDoneCandidate.importStatus
        )

        static let triageRowDoneFailed = triageRow(
            for: importTabFailedCandidate,
            placement: .done,
            skipAction: nil,
            matched: triageMatch(
                releaseId: "rel-failed",
                title: importTabFailedCandidate.displayName,
                trackCount: 18,
                source: .discogs,
                signal: .barcode
            ),
            selectable: false,
            importStatus: importTabFailedCandidate.importStatus
        )

        private static let triageGroupedRows = importTabGroupedCandidates.map {
            triageRow(
                for: $0,
                placement: .needsYou(
                    group: .noMatch,
                    reason: .disagreement(disagreement: .noMatch)
                ),
                skipAction: .skip,
                matched: nil,
                selectable: false
            )
        }

        static let importTabCandidates =
            [
                importTabCandidate,
                importTabSeveralMatchesCandidate,
                importTabConflictCandidate,
                importTabTrackMismatchCandidate,
                importTabAlreadyInLibraryCandidate,
                importTabNoMatchCandidate,
                importTabIdentifyingCandidate,
                importTabImportingCandidate,
                importTabDoneCandidate,
                importTabFailedCandidate,
                folderCandidates[1],
            ] + importTabGroupedCandidates

        private static let triageImportQueue: BridgeTriageQueue = {
            BridgeTriageQueue(
                sections: [
                    BridgeTriageSection(
                        tab: .pending,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [
                            triageRowReady,
                            triageRowPickAPressing,
                            triageRowSignalsConflict,
                            triageRowTrackMismatch,
                            triageRowAlreadyInLibrary,
                            triageRowNoMatch,
                            triageRowStillIdentifying,
                            triageRowImporting,
                        ]
                        .map(candidateEntry)
                    ),
                    BridgeTriageSection(
                        tab: .pending,
                        watchedFolderPath: importWatchedFolder.path,
                        group: BridgeTriageGroup(
                            key: BridgeFolderReleaseDecisionKey(
                                watchedFolderPath: importWatchedFolder.path,
                                relativeFolderPath: "Artist Collection"
                            ),
                            name: "Artist Collection"
                        ),
                        entries: triageGroupedRows.map(candidateEntry)
                    ),
                    BridgeTriageSection(
                        tab: .pending,
                        watchedFolderPath: importWatchedFolder.path,
                        group: BridgeTriageGroup(
                            key: folderReleaseBoundary.key,
                            name: folderReleaseBoundary.name
                        ),
                        entries: [
                            boundaryEntry(folderReleaseBoundary)
                        ]
                    ),
                    BridgeTriageSection(
                        tab: .done,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [
                            triageRowDoneImported,
                            triageRowDoneFailed,
                        ]
                        .map(candidateEntry)
                    ),
                    BridgeTriageSection(
                        tab: .skipped,
                        watchedFolderPath: importWatchedFolder.path,
                        group: nil,
                        entries: [candidateEntry(triageRowSkipped)]
                            + invalidCandidates.map(invalidEntry)
                    ),
                ],
                counts: BridgeTriageTabCounts(
                    pending: 11,
                    done: 2,
                    skipped: 1 + UInt32(invalidCandidates.count)
                ),
                folderScanStatuses: []
            )
        }()

        /// One store for the sidebar and whole Import-tab previews. Every
        /// candidate row resolves to the candidate the detail pane opens, while
        /// boundary and invalid entries exercise the two non-candidate shapes.
        @MainActor
        static func importTabStore() -> ImportStore {
            let s = ImportStore()
            s.watchedFolders = [importWatchedFolder]
            for candidate in importTabCandidates {
                s.folderCandidates[candidate.key] = candidate
            }
            s.triageQueue = triageImportQueue
            s.queueIdentifyProgress = (identified: 112, total: 130)
            return s
        }

        static func importTabImporter() -> Importer {
            let mapping = awaitingPickTable
            let optionsBySheet = sheetBindingOptions
            return Importer(
                sheetBindingOptions: { _, sheetFileId in
                    guard let options = optionsBySheet[sheetFileId] else {
                        throw StubError.notImplemented
                    }
                    return options
                },
                candidateMapping: { _ in mapping }
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
                    localPath: localPath ?? "/tmp/fake/\(name)"
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
            isCover: Bool = false,
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
                role: isCover
                    ? .cover(choice: choice) : .artwork(choice: choice),
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
                binding: .describes(fileId: "Album Title.flac"),
                trackCount: 9
            ),
            becomes: .slots(first: 1, last: 9)
        )

        /// What core offers a sheet in this folder: the FLAC it can use, and
        /// the MP3 it can't, refused with its codec named.
        static let sheetBindingOptions: [String: [BridgeSheetBindingOption]] = [
            "Album Title.cue": [
                BridgeSheetBindingOption(
                    fileId: "Album Title.flac",
                    offer: .offered
                ),
                BridgeSheetBindingOption(
                    fileId: "Album Title.mp3",
                    offer: .refusedCodec(codec: "MP3")
                ),
            ]
        ]

        static let bridgeCandidateFiles = BridgeCandidateFiles(
            files: [
                previewImage(name: "Back.png", size: 1_800_000),
                boundTrackSheet,
                previewFile(
                    name: "Album Title.flac",
                    size: 340_000_000,
                    role: .audio,
                    becomes: .slots(first: 1, last: 9)
                ),
                previewImage(name: "Front.png", size: 2_500_000, isCover: true),
                previewImage(name: "Matrix.png", size: 900_000),
                previewFile(name: "info.log", size: 6000, role: .document),
                previewFile(name: "rip.nfo", size: 400, role: .other),
            ]
                + (1...14)
                .map { i in
                    previewImage(
                        name: "scan-\(i).jpg",
                        size: 1_400_000,
                        dirPrefix: "scans/"
                    )
                },
            formatLabel: "CUE+FLAC",
            collapsedDirectories: []
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

        /// The picked release’s editor seed, as the decided-identity answer carries it:
        /// projected from the release the way the commit worker maps it, so every
        /// credited album artist is present.
        static let releaseSeedBridge: BridgeReleaseUserEdit = {
            let tracks: [BridgeTrackUserEdit] = (1...9)
                .map { i in
                    BridgeTrackUserEdit(
                        title: "Track Title \(i)",
                        side: 1,
                        trackNumber: Int32(i),
                        artistNames: i == 5 ? ["Featured Artist"] : [],
                        file: .standalone(fileId: "Track \(i).flac")
                    )
                }
            return BridgeReleaseUserEdit(
                albumTitle: "Album Title One",
                albumArtistNames: ["Artist Name"],
                pressing: BridgePressingEdit(
                    year: 1996,
                    format: "CD",
                    label: "Label Name",
                    catalogNumber: "6006-2",
                    country: "US",
                    barcode: nil
                ),
                tracks: tracks
            )
        }()

        /// Editor seed for the confirming previews — the raw release edit the
        /// prefetch's seed projects into.
        static let confirmEditValues: BridgeRawReleaseEdit =
            rawReleaseEditFromUserEdit(
                edit: releaseSeedBridge,
                trackIdPrefix: "import-track"
            )

        /// What picking `releaseDetailBridge` claims: the pressing itself, so
        /// the header states no separate metadata source.
        static let claimBridge = BridgeClaimLine(
            choice: .exact(
                releaseId: releaseDetailBridge.releaseId,
                source: releaseDetailBridge.source
            ),
            level: .exact,
            evidence: .discIdAlone,
            release: "CD \u{00b7} 1996 \u{00b7} US \u{00b7} 6006-2",
            trackCount: releaseDetailBridge.trackCount
        )

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

        static let coverImage = previewImage(
            name: "Front.png",
            size: 2_500_000,
            isCover: true
        )

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

        static let candidateFilesTracks = BridgeCandidateFiles(
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
            formatLabel: "FLAC",
            collapsedDirectories: []
        )

    }
#endif
