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

        /// A `BridgeTriageRow` fixture keyed to an existing `Candidate` fixture
        /// (`folderImportStore`'s roster), so the sidebar and the detail pane
        /// agree on the same folder.
        static func triageRow(
            for candidate: Candidate,
            placement: BridgeTriagePlacement,
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
                matched: matched,
                selectable: selectable,
                importStatus: importStatus,
                picked: picked,
                claim: claim
            )
        }

        static let triageRowReady = BridgeTriageRow(
            candidateKey: "/Music/Downloads/1997 - album title (192 kbps)",
            folderName: "1997 - album title (192 kbps)",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "1997 - album title (192 kbps)",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .ready,
            matched: triageMatch(releaseId: "rel-ready", title: "Album Title"),
            selectable: true,
            importStatus: nil,
            // Ready means identification settled on one match, which is a
            // pick — the record a bulk import of this row commits from.
            picked: .release(
                source: .musicBrainz,
                releaseId: "rel-ready",
                claim: .exact
            ),
            claim: .exact(releaseId: "rel-ready", source: .musicBrainz)
        )

        static let triageRowPickAPressing = BridgeTriageRow(
            candidateKey: "/Music/Downloads/1966 - album title five",
            folderName: "1966 - album title five",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "1966 - album title five",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .needsYou(
                group: .pickAPressing,
                reason: .disagreement(
                    disagreement: .severalMatches(count: 4)
                )
            ),
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
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowSignalsConflict = BridgeTriageRow(
            candidateKey: "/Music/Downloads/album title six - remaster",
            folderName: "album title six - remaster",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "album title six - remaster",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .needsYou(
                group: .signalsDisagree,
                reason: .disagreement(disagreement: .signalsConflict)
            ),
            matched: nil,
            selectable: false,
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowAlreadyInLibrary = BridgeTriageRow(
            candidateKey:
                "/Music/Downloads/artist name - album ti\u{2026}INT 846.104 germany",
            folderName:
                "artist name - album ti\u{2026}INT 846.104 germany",
            watchedFolderPath: "/Music/Downloads",
            displayPath:
                "artist name - album ti\u{2026}INT 846.104 germany",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .needsYou(
                group: .alreadyInLibrary,
                reason: .disagreement(disagreement: .alreadyInLibrary)
            ),
            matched: triageMatch(
                releaseId: "rel-reissue",
                title: "Album Title (Reissue)",
                year: 2004,
                trackCount: 14,
                signal: .barcode
            ),
            selectable: false,
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowNoMatch = BridgeTriageRow(
            candidateKey: "/Media/rips/album title seven (bootleg)",
            folderName: "album title seven (bootleg)",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "album title seven (bootleg)",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .needsYou(
                group: .noMatch,
                reason: .disagreement(disagreement: .noMatch)
            ),
            matched: nil,
            selectable: false,
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowStillIdentifying = BridgeTriageRow(
            candidateKey: "/Music/Downloads/1972 - album title (192 kbps)",
            folderName: "1972 - album title (192 kbps)",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "1972 - album title (192 kbps)",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .needsYou(
                group: .stillIdentifying,
                reason: .stillIdentifying(phase: .running)
            ),
            matched: nil,
            selectable: false,
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowSkipped = BridgeTriageRow(
            candidateKey: "/Music/Downloads/Album Title Two",
            folderName: "Album Title Two",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "Album Title Two",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .skipped,
            matched: nil,
            selectable: false,
            importStatus: nil,
            picked: nil,
            claim: nil
        )

        static let triageRowDoneImported = BridgeTriageRow(
            candidateKey: "/Music/Downloads/Album Title Ten",
            folderName: "Album Title Ten",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "Album Title Ten",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .done,
            matched: triageMatch(
                releaseId: "rel-ten",
                title: "Album Title Ten",
                year: 1995,
                trackCount: 10
            ),
            selectable: false,
            importStatus: .complete(
                releaseId: "rel-ten",
                albumId: "album-ten"
            ),
            picked: nil,
            claim: nil
        )

        static let triageRowDoneFailed = BridgeTriageRow(
            candidateKey: "/Music/Downloads/Album Title Twelve",
            folderName: "Album Title Twelve",
            watchedFolderPath: "/Music/Downloads",
            displayPath: "Album Title Twelve",
            resolvedBoundaries: [],
            combineAncestorKey: nil,
            actionable: true,
            placement: .done,
            matched: triageMatch(
                releaseId: "rel-twelve",
                title: "Album Title Twelve",
                year: 2011,
                trackCount: 9,
                source: .discogs,
                signal: .barcode
            ),
            selectable: false,
            importStatus: .error(
                error: .Diagnostic(
                    category: .import,
                    detail: "track 7 is truncated"
                )
            ),
            picked: nil,
            claim: nil
        )

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
                            triageRowAlreadyInLibrary,
                            triageRowNoMatch,
                            triageRowStillIdentifying,
                        ]
                        .map(candidateEntry)
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
                        entries: [
                            candidateEntry(triageRowSkipped),
                            invalidEntry(invalidCandidates[0]),
                        ]
                    ),
                ],
                counts: BridgeTriageTabCounts(
                    pending: 7,
                    done: 2,
                    skipped: 1 + UInt32(invalidCandidates.count)
                ),
                folderScanStatuses: []
            )
        }()

        /// Seeded ImportStore for the standalone sidebar preview
        /// (`ImportCandidateListContent`) — one row per state the row view
        /// renders differently, so the preview exercises every tab and every
        /// Needs-you group. Independent of `folderImportStore`'s roster; no
        /// detail pane sits behind this preview, so the keys don't need to
        /// match a `Candidate`.
        @MainActor
        static func triageImportStore() -> ImportStore {
            let s = ImportStore()
            s.watchedFolders = [importWatchedFolder]
            s.triageQueue = triageImportQueue
            s.queueIdentifyProgress = (identified: 112, total: 130)
            return s
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
