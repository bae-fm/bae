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
            source: BridgeCatalog = .musicBrainz,
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
            identification: BridgeIdentificationStatus? = nil,
            skipAction: BridgeTriageSkipAction?,
            actions: [BridgeCandidateAction],
            matched: BridgeMatchedRelease?,
            metadataSummary: BridgeTriageMetadataSummary? = nil,
            coverThumbnail: BridgeCoverImageSource? = nil,
            importStatus: BridgeTriageImportStatus? = nil,
            metadataProvenance: BridgeMetadataProvenance? = nil,
            reading: BridgeTriageReading = .unidentified
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
                identification: identification,
                skipAction: skipAction,
                actions: actions,
                matched: matched,
                metadataSummary: metadataSummary,
                coverThumbnail: coverThumbnail,
                selectable: actions.contains(.importReady),
                importStatus: importStatus,
                metadataProvenance: metadataProvenance,
                reading: reading
            )
        }

        /// A preview candidate showing `identifyState`. Nothing is running in
        /// a preview, so the state stands as the one its stored verdict would
        /// resume — which is what every surface falls back to.
        static func importTabFolder(
            path: String,
            name: String,
            trackCount: UInt32 = 9,
            identifyState: IdentifyState
        ) -> Candidate {
            var candidate = Candidate(
                bridge: BridgeFolderCandidate(
                    compositionAction: .combine,
                    combination: nil,
                    sourceFileEditsAllowed: true,
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
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 12,
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                title: "Album Title Seven",
                artist: "Artist Name",
                label: "Label Name",
                coverArt: nil,
                sources: [
                    BridgeReleaseGroupSource(
                        source: .musicBrainz,
                        groupUrl:
                            "https://musicbrainz.org/release-group/group-track-mismatch"
                    )
                ],
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
                run: identifyRunFound,
                groups: [trackMismatchGroup],
                libraryStatuses: [:],
                trackCount: 1,
                agreements: [:],
                narrowedOut: .nothing,
                catalogAgreements: []
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
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [
                    releaseDetailBridge.releaseId:
                        importTabAlreadyInLibraryStatus
                ],
                trackCount: 14,
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
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
                compositionAction: .combine,
                combination: nil,
                sourceFileEditsAllowed: true,
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
                record: BridgeMetadataRef(
                    catalog: releaseDetailBridge.source,
                    key: releaseDetailBridge.releaseId
                ),
                partners: []
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

        private static let importTabUnidentifiedCandidate = importTabFolder(
            path: "Release Folder Eleven",
            name: "Release Folder Eleven",
            identifyState: searchStateNotFound.identifyState
        )

        private static let importTabTaggedCandidate = importTabFolder(
            path: "Release Folder Twelve",
            name: "Release Folder Twelve",
            identifyState: searchStateNotFound.identifyState
        )

        private static let importTabIdentifiedCandidate = importTabFolder(
            path: "Release Folder Thirteen",
            name: "Release Folder Thirteen",
            identifyState: .found(
                run: identifyRunFound,
                groups: [searchGroupExact],
                libraryStatuses: [:],
                trackCount: 9,
                agreements: searchAgreementsExact,
                narrowedOut: .nothing,
                catalogAgreements: catalogAgreements
            )
        )

        private static let importTabIdentifiedSeveralMatchesCandidate =
            importTabFolder(
                path: "Release Folder Fourteen",
                name: "Release Folder Fourteen",
                identifyState: .found(
                    run: identifyRunFound,
                    groups: [searchGroupExact],
                    libraryStatuses: [:],
                    trackCount: 9,
                    agreements: searchAgreementsExact,
                    narrowedOut: .nothing,
                    catalogAgreements: catalogAgreements
                )
            )

        /// Nothing written about the release yet: the row is its folder, and
        /// the folder's own image is still its cover.
        static let triageRowUnidentified = triageRow(
            for: importTabUnidentifiedCandidate,
            placement: .pending,
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .skip],
            matched: nil,
            metadataSummary: nil,
            coverThumbnail: .local(path: previewArtPath("Front.png"))
        )

        /// A draft read off the folder's own metadata — a title and an artist, and no
        /// source to name.
        static let triageRowPrefilledFromTags = triageRow(
            for: importTabTaggedCandidate,
            placement: .pending,
            skipAction: .skip,
            actions: [.identify, .clearMetadata, .skip],
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Twelve",
                albumArtistAssignments: [newArtist("Artist Name")]
            ),
            coverThumbnail: .local(path: previewArtPath("Front.png")),
            metadataProvenance: .fileMetadata,
            reading: .prefilled
        )

        /// Both catalogs a pick paired, each with its own page for the
        /// pressing. The draft was read from the MusicBrainz one.
        static let identifiedFromBothCatalogs = [
            BridgeReleaseRecord(
                catalog: .musicBrainz,
                url: "https://musicbrainz.org/release/rel-paired"
            ),
            BridgeReleaseRecord(
                catalog: .discogs,
                url: "https://www.discogs.com/release/discogs-paired"
            ),
        ]

        /// A pick that paired two sources' releases into one pressing: the row
        /// names both.
        static let triageRowIdentifiedOnline = triageRow(
            for: importTabIdentifiedCandidate,
            placement: .ready,
            skipAction: .skip,
            actions: [
                .importReady, .identify, .resetToFileMetadata, .clearMetadata,
                .skip,
            ],
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Thirteen",
                albumArtistAssignments: [newArtist("Artist Name")]
            ),
            coverThumbnail: .local(path: previewArtPath("Front.png")),
            metadataProvenance: .externalRelease(
                record: BridgeMetadataRef(
                    catalog: .musicBrainz,
                    key: "rel-paired"
                ),
                partners: [
                    BridgeMetadataRef(
                        catalog: .discogs,
                        key: "discogs-paired"
                    )
                ]
            ),
            reading: .identified(records: identifiedFromBothCatalogs)
        )

        /// Identified, and still asked which of several pressings it is:
        /// the row names its sources and carries the question at once.
        static let triageRowIdentifiedSeveralMatches = triageRow(
            for: importTabIdentifiedSeveralMatchesCandidate,
            placement: .needsYou(
                reason: .severalMatches(count: 2)
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Fourteen",
                albumArtistAssignments: [newArtist("Artist Name")]
            ),
            coverThumbnail: .local(path: previewArtPath("Front.png")),
            metadataProvenance: .externalRelease(
                record: BridgeMetadataRef(
                    catalog: .musicBrainz,
                    key: "rel-several"
                ),
                partners: [
                    BridgeMetadataRef(
                        catalog: .discogs,
                        key: "discogs-several"
                    )
                ]
            ),
            reading: .identified(records: identifiedFromBothCatalogs)
        )

        @MainActor
        static let triageRowReady = triageRow(
            for: importTabCandidate,
            placement: .ready,
            skipAction: .skip,
            actions: [
                .importReady, .identify, .resetToFileMetadata, .clearMetadata,
                .skip,
            ],
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: releaseDetailBridge.title,
                artist: releaseDetailBridge.artist,
                year: releaseDetailBridge.year,
                format: releaseDetailBridge.format,
                trackCount: releaseDetailBridge.trackCount
            ),
            metadataSummary: nil,
            metadataProvenance: .externalRelease(
                record: BridgeMetadataRef(
                    catalog: releaseDetailBridge.source,
                    key: releaseDetailBridge.releaseId
                ),
                partners: []
            )
        )

        static let triageRowPickAPressing = triageRow(
            for: importTabSeveralMatchesCandidate,
            placement: .needsYou(
                reason: .severalMatches(count: 2)
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
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
            importStatus: nil
        )

        /// Two signals that named different releases: the row asks the same
        /// question any multi-match does.
        static let triageRowSeveralMatchesFromSignals = triageRow(
            for: importTabDisagreementCandidate,
            placement: .needsYou(
                reason: .severalMatches(count: 2)
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowTrackMismatch = triageRow(
            for: importTabTrackMismatchCandidate,
            placement: .needsYou(
                reason: .trackCountDisagrees(local: 1, source: 10)
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
            matched: triageMatch(
                releaseId: "rel-track-mismatch",
                title: "Album Title Seven",
                year: 1994,
                trackCount: 10
            ),
            metadataSummary: nil
        )

        static let triageRowAlreadyInLibrary = triageRow(
            for: importTabAlreadyInLibraryCandidate,
            placement: .needsYou(
                reason: .alreadyInLibrary
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: "Album Title (Reissue)",
                year: 2004,
                trackCount: 14,
                signal: .barcode
            ),
            metadataSummary: nil
        )

        static let triageRowNoMatch = triageRow(
            for: importTabNoMatchCandidate,
            placement: .needsYou(
                reason: .noMatch
            ),
            skipAction: .skip,
            actions: [.identify, .resetToFileMetadata, .clearMetadata, .skip],
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowIdentifying = triageRow(
            for: importTabIdentifyingCandidate,
            placement: .pending,
            identification: .running,
            skipAction: .skip,
            actions: [.skip],
            matched: nil,
            metadataSummary: nil
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
            actions: [],
            matched: triageMatch(
                releaseId: "rel-importing",
                title: importTabImportingCandidate.displayName,
                trackCount: 15
            ),
            metadataSummary: nil,
            importStatus: .importing
        )

        static let triageRowSkipped = triageRow(
            for: folderCandidates[1],
            placement: .skipped,
            skipAction: .unskip,
            actions: [.restore],
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowDoneImported = triageRow(
            for: importTabDoneCandidate,
            placement: .done,
            skipAction: nil,
            actions: [],
            matched: triageMatch(
                releaseId: "preview-release",
                title: importTabDoneCandidate.displayName,
                trackCount: 5
            ),
            metadataSummary: nil,
            importStatus: .complete(
                releaseId: "preview-release",
                albumId: "preview-album"
            )
        )

        static let triageRowFailed = triageRow(
            for: importTabFailedCandidate,
            placement: .failed,
            skipAction: nil,
            actions: [.identify, .resetToFileMetadata, .clearMetadata],
            matched: triageMatch(
                releaseId: "rel-failed",
                title: importTabFailedCandidate.displayName,
                trackCount: 18,
                source: .discogs,
                signal: .barcode
            ),
            metadataSummary: nil,
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
                actions: [
                    .importReady, .identify, .resetToFileMetadata,
                    .clearMetadata,
                    .skip,
                ],
                matched: triageMatch(
                    releaseId: releaseDetailBridge.releaseId,
                    title: releaseDetailBridge.title,
                    artist: releaseDetailBridge.artist,
                    year: releaseDetailBridge.year,
                    format: releaseDetailBridge.format,
                    trackCount: releaseDetailBridge.trackCount
                ),
                metadataSummary: nil,
                metadataProvenance: .externalRelease(
                    record: BridgeMetadataRef(
                        catalog: releaseDetailBridge.source,
                        key: releaseDetailBridge.releaseId
                    ),
                    partners: []
                )
            ),
            triageRow(
                for: importTabGroupedCandidates[1],
                placement: .needsYou(
                    reason: .noMatch
                ),
                skipAction: .skip,
                actions: [
                    .identify, .resetToFileMetadata, .clearMetadata, .skip,
                ],
                matched: nil,
                metadataSummary: nil
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
            triageRowIdentifying,
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
                candidateKey: triageRowIdentifying.candidateKey,
                stableKey:
                    "candidate:\(triageRowIdentifying.candidateKey)",
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
            store.identificationProgress = (identified: 112, total: 130)
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
            let importingKey = importTabImportingCandidate.key
            let inFlight = importTabImportInFlight
            return Importer(
                candidateRuntime: { key in
                    guard key == importingKey else { return nil }
                    return BridgeCandidateRuntimeSnapshot(
                        identifyState: .idle,
                        signalsToolbar: BridgeSignalsToolbar(signals: []),
                        import: inFlight,
                        search: nil
                    )
                }
            )
        }

    }
#endif
