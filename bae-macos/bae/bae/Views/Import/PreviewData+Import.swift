#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the Import flow: watched folders and folder
    // candidates, the seeded import store, candidate file listings (CUE+FLAC
    // and per-track), the picked-release detail/seed and its confirm edit, and
    // the identify/search states (exact, manual, conflict, triangulating,
    // not-found) with their signal toolbars. Generic placeholder names
    // throughout.
    extension PreviewData {
        static let importWatchedFolder = BridgeWatchedFolder(
            path: "/Music/Downloads",
            name: "Downloads"
        )

        /// Seeded ImportStore for the ImportView whole-view preview — the
        /// watched folder plus every folder candidate. ImportStore is a non-Sendable
        /// `@Observable`, so it needs `@MainActor` isolation to hold as a static.
        @MainActor
        static let folderImportStore: ImportStore = {
            let s = ImportStore()
            s.watchedFolders = [importWatchedFolder]
            for candidate in folderCandidates {
                s.folderCandidates[candidate.key] = candidate
            }
            return s
        }()

        static let folderCandidates: [Candidate] = [
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Album Title One",
                sourceFolderName: "Album Title One",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 9,
                skipped: false,
                isAdded: false,
            ),
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Album Title Two [Label CAT-002]",
                sourceFolderName: "Album Title Two",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 12,
                // Skipped example — renders under the Skipped tab.
                skipped: true,
                isAdded: false,
            ),
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Compilation Vol. 3",
                sourceFolderName: "Compilation Vol. 3",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 15,
                skipped: false,
                isAdded: false,
            ),
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/EP Release",
                sourceFolderName: "EP Release",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 5,
                skipped: false,
                isAdded: false,
            ),
            BridgeFolderCandidate(
                folderPath: "/Music/Downloads/Live Recording 2023",
                sourceFolderName: "Live Recording 2023",
                watchedFolderPath: "/Music/Downloads",
                files: bridgeCandidateFiles,
                trackCount: 18,
                // Added example (content-hash match) — renders under the Added tab.
                skipped: false,
                isAdded: true,
            ),
        ]
        .map(Candidate.init(bridge:))

        static let importStatuses: [String: BridgeCandidateImportStatus] = [
            "/Music/Downloads/Compilation Vol. 3": .importing(
                progressPercent: 45,
                step: .running(phase: .measuringLoudness)
            ),
            // A completed import — tabs under Added via its import status.
            "/Music/Downloads/EP Release": .complete(
                releaseId: "preview-release",
                albumId: "preview-album"
            ),
        ]

        /// Folders that look like a release but failed validation — surface under
        /// the Skipped tab with a warning and reason.
        static let invalidCandidates: [BridgeInvalidCandidate] = [
            BridgeInvalidCandidate(
                folderPath: "/Music/Downloads/Broken Rip",
                sourceFolderName: "Broken Rip",
                watchedFolderPath: "/Music/Downloads",
                reason: .corruptAudioFile(path: "03.flac")
            )
        ]

        private static func previewFile(
            name: String,
            size: UInt64,
            role: BridgeFileRole
        ) -> BridgeCandidateFile {
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: name,
                    size: size,
                    dirPrefix: nil,
                    fileName: name,
                    localPath: "/tmp/fake/\(name)"
                ),
                role: role
            )
        }

        private static func previewImage(
            name: String,
            size: UInt64,
            isCover: Bool = false
        ) -> BridgeCandidateFile {
            let choice = BridgeCoverChoice(
                selection: .releaseImage(fileId: name),
                previewSource: .local(path: "/tmp/fake/\(name)"),
                thumbnailSource: .local(path: "/tmp/fake/\(name)")
            )
            return previewFile(
                name: name,
                size: size,
                role: isCover
                    ? .cover(choice: choice) : .artwork(choice: choice)
            )
        }

        /// A sheet bound to `Album Title.flac`.
        static let boundTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(
                binding: .describes(fileId: "Album Title.flac"),
                trackCount: 9
            )
        )

        /// A sheet whose `FILE` directive names audio that isn't in the folder.
        static let unboundTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(binding: .unresolved, trackCount: 9)
        )

        /// A sheet bound to audio bae can't carve tracks out of.
        static let refusedTrackSheet = previewFile(
            name: "Album Title.cue",
            size: 1200,
            role: .trackSheet(
                binding: .refusedCodec(fileId: "Album Title.mp3", codec: "MP3"),
                trackCount: 9
            )
        )

        static let bridgeCandidateFiles = BridgeCandidateFiles(
            files: [
                previewImage(name: "Back.png", size: 1_800_000),
                boundTrackSheet,
                previewFile(
                    name: "Album Title.flac",
                    size: 340_000_000,
                    role: .audio
                ),
                previewImage(name: "Front.png", size: 2_500_000, isCover: true),
                previewImage(name: "Matrix.png", size: 900_000),
                previewFile(name: "info.log", size: 6000, role: .document),
                previewFile(name: "rip.nfo", size: 400, role: .other),
            ],
            formatLabel: "CUE+FLAC"
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
                        side: 1,
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
                trackCountMismatch: false,
                tracks: tracks,
                coverArt: [],
                defaultCover: nil,
            )
        }()

        /// The picked release's editor seed, as `prefetchRelease` returns it:
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
                    barcode: nil,
                ),
                tracks: tracks,
            )
        }()

        /// Editor seed for the confirming previews — the raw release edit the
        /// prefetch's seed projects into.
        static let confirmEditValues: BridgeRawReleaseEdit =
            rawReleaseEditFromUserEdit(
                edit: releaseSeedBridge,
                trackIdPrefix: "import-track"
            )

        /// The claim a disc-ID match on `releaseDetailBridge` produces: the
        /// pressing itself, so the header states no separate metadata source.
        static let claimBridge = BridgeClaimLine(
            choice: .exact(
                releaseId: releaseDetailBridge.releaseId,
                source: releaseDetailBridge.source
            ),
            evidence: .discIdAlone,
            release: "CD \u{00b7} 1996 \u{00b7} US \u{00b7} 6006-2",
            trackCount: releaseDetailBridge.trackCount,
            showsMetadataSource: false
        )

        /// Per-track audio candidate (nine FLAC files) plus one cover image, two
        /// documents, and a sheet describing nothing yet — the file-per-track
        /// counterpart to `bridgeCandidateFiles`.
        static let candidateFilesTracks = BridgeCandidateFiles(
            files: (1...9)
                .map { i in
                    previewFile(
                        name: "Track \(i).flac",
                        size: UInt64(35_000_000 + i * 2_000_000),
                        role: .audio
                    )
                }
                + [
                    previewImage(
                        name: "Front.png",
                        size: 2_500_000,
                        isCover: true
                    ),
                    previewFile(name: "info.log", size: 6000, role: .document),
                    previewFile(name: "notes.txt", size: 1200, role: .document),
                    previewFile(
                        name: "Album.cue",
                        size: 1100,
                        role: .trackSheet(binding: .unresolved, trackCount: 9)
                    ),
                ],
            formatLabel: "FLAC"
        )

        // MARK: - Import search

        /// Two pressings of one release group — the exact-match results state.
        static let exactPressings: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-123",
                year: 1996,
                format: "CD",
                label: "Label Name",
                catalogNumber: "6006-2",
                country: "US",
            ),
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-456",
                year: 1988,
                format: "CD",
                label: "Label Name",
                catalogNumber: "1871-2",
                country: "US",
            ),
        ]

        static let searchGroupExact = ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "group-preview",
                title: "Album Title",
                artist: "Artist Name",
                coverArt: nil,
                sourceLabel: "MusicBrainz",
                groupUrl: "https://musicbrainz.org/release-group/group-preview",
                yearMin: 1988,
                yearMax: 1996,
                pressings: exactPressings,
            )
        )

        static let searchProvenanceExact: [String: BridgeResultProvenance] =
            Dictionary(
                uniqueKeysWithValues: exactPressings.map {
                    (
                        $0.releaseId,
                        BridgeResultProvenance(
                            byDiscId: true,
                            byBarcode: false,
                            matchesCatalog: true
                        )
                    )
                }
            )

        /// Two distinct release groups — the manual-search results state.
        static let searchGroupsManual: [ReleaseGroup] = [
            ReleaseGroup(
                bridge: BridgeReleaseGroup(
                    id: "grp-1",
                    title: "Album Title One",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: "https://musicbrainz.org/release-group/grp-1",
                    yearMin: 1996,
                    yearMax: 1996,
                    pressings: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-aaa",
                            year: 1996,
                            format: "CD",
                            label: "Label Name",
                            catalogNumber: "6006-2",
                            country: "US",
                        ),
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-bbb",
                            year: 1996,
                            format: "CD",
                            label: "Another Label",
                            catalogNumber: "AL-1234",
                            country: "JP",
                        ),
                    ],
                )
            ),
            ReleaseGroup(
                bridge: BridgeReleaseGroup(
                    id: "grp-2",
                    title: "Album Title One (Remaster)",
                    artist: "Artist Name",
                    coverArt: nil,
                    sourceLabel: "MusicBrainz",
                    groupUrl: "https://musicbrainz.org/release-group/grp-2",
                    yearMin: 2005,
                    yearMax: 2005,
                    pressings: [
                        BridgeMetadataResult(
                            source: .musicBrainz,
                            releaseId: "rel-ccc",
                            year: 2005,
                            format: "CD",
                            label: "Reissue Records",
                            catalogNumber: "RR-500",
                            country: "EU",
                        )
                    ],
                )
            ),
        ]

        /// disc-id vs barcode candidate lists — the conflict results state.
        static let conflictDiscidResults: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-disc-1",
                year: 1996,
                format: "CD",
                label: "Label A",
                catalogNumber: "AAA-001",
                country: "US",
            )
        ]

        static let conflictBarcodeResults: [BridgeMetadataResult] = [
            BridgeMetadataResult(
                source: .musicBrainz,
                releaseId: "rel-bar-1",
                year: 2001,
                format: "CD",
                label: "Label B",
                catalogNumber: "BBB-002",
                country: "JP",
            )
        ]

        /// Settled OCR/text signals — catalogs plus cover free-text.
        static let settledSignals = Signals(
            text: .settled(
                catalogs: ["WPCR-80001"],
                freeText: [
                    "Artist Name",
                    "Album Title",
                    "Label Records JP - WPCR-80001",
                    "Recorded at Studio A",
                    "Produced by Producer Name",
                ]
            )
        )

        // MARK: - Signals toolbar

        /// Both identity signals still looking up, one catalog filter present.
        static let toolbarBothRunning = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .lookingUp,
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 0),
                excluded: false
            ),
        ])

        /// Disc-id settled, barcode still running, two catalog filters (one
        /// confirming, one not).
        static let toolbarOneSettled = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 3),
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 1),
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "A2 16018",
                origin: .textFile,
                state: .confirms(count: 0),
                excluded: false
            ),
        ])

        /// Barcode excluded from triangulation while both identity signals matched.
        static let toolbarBarcodeExcluded = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .found(count: 4),
                excluded: true
            ),
            BridgeToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 0),
                excluded: false
            ),
        ])

        /// Both identity signals matched but disagree — the conflict toolbar.
        static let toolbarConflictBothMatched = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "5051961234567",
                origin: .artwork,
                state: .found(count: 3),
                excluded: false
            ),
        ])

        /// Identify skipped — both identity signals have no value and are skipped.
        static let toolbarSkippedNoSignals = BridgeSignalsToolbar(signals: [
            BridgeToolbarSignal(
                kind: .discId,
                role: .identity,
                value: nil,
                origin: .discToc,
                state: .skipped,
                excluded: false
            ),
            BridgeToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: nil,
                origin: .artwork,
                state: .skipped,
                excluded: false
            ),
        ])

        /// Exact-match display state: disc-id found one group, catalog confirms it.
        static let searchStateFoundExact = ImportSearchState(
            identifyState: .found(
                group: searchGroupExact,
                libraryStatuses: [:],
                trackCount: 0,
                provenance: searchProvenanceExact,
            ),
            showManualSearch: false,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: settledSignals,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 3),
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .catalog,
                    role: .filter,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .confirms(count: 1),
                    excluded: false
                ),
            ]),
        )

        /// Manual-search display state: results listed, the form open.
        static let searchStateManual = ImportSearchState(
            identifyState: .found(
                group: searchGroupsManual[0],
                libraryStatuses: [:],
                trackCount: 0,
                provenance: [:],
            ),
            showManualSearch: true,
            error: nil,
            searchGroups: searchGroupsManual,
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: true,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: settledSignals,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .catalog,
                    role: .filter,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .confirms(count: 0),
                    excluded: false
                ),
            ]),
        )

        /// Conflict display state: disc-id and barcode disagree on identity.
        static let searchStateConflict = ImportSearchState(
            identifyState: .conflict(
                discidResults: conflictDiscidResults,
                discidLibraryStatuses: [:],
                barcodeResults: conflictBarcodeResults,
                barcodeLibraryStatuses: [:],
                matchedBarcode: "5051961234567",
                trackCount: 11,
            ),
            showManualSearch: false,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "5051961234567",
                    origin: .artwork,
                    state: .found(count: 3),
                    excluded: false
                ),
            ]),
        )

        /// Auto-lookup in progress: disc-id looking up, barcode skipped.
        static let searchStateTriangulating = ImportSearchState(
            identifyState: .triangulating(
                discid: .lookingUp,
                barcode: .skipped,
            ),
            showManualSearch: false,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: false,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .lookingUp,
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: nil,
                    origin: .artwork,
                    state: .skipped,
                    excluded: false
                ),
            ]),
        )

        /// Manual search after both signals came up empty.
        static let searchStateNotFound = ImportSearchState(
            identifyState: .notFoundAnywhere,
            showManualSearch: true,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .noMatch,
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "5051961234567",
                    origin: .artwork,
                    state: .noMatch,
                    excluded: false
                ),
            ]),
        )
    }
#endif
