#if DEBUG
    import AppKit
    import BaeKit
    import Foundation

    /// Preview fixtures for the import candidate list and its rows.
    extension PreviewData {
        // MARK: - Triage sidebar

        /// A `BridgeMatchedRelease` fixture with a settled single pressing.
        static func triageMatch(
            releaseId: String,
            title: String,
            artist: String? = "Artist Name",
            year: Int32? = 1997,
            media: [BridgeMediaCount] = PreviewData.media(.cd),
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
                    media: media,
                    trackCount: trackCount
                ),
                // A path `PreviewData.artImageStore()` serves from disk.
                cover: BridgeRemoteImageSet(
                    url: previewArtPath(title),
                    downscaled: []
                ),
                evidence: BridgeMatchEvidence(source: source, signal: signal)
            )
        }

        /// A `BridgeTriageRow` fixture for an existing `Candidate` fixture.
        static func triageRow(
            for candidate: Candidate,
            placement: BridgeTriagePlacement,
            matched: BridgeMatchedRelease?,
            metadataSummary: BridgeTriageMetadataSummary? = nil,
            cover: BridgeCoverImageSource? = nil,
            importStatus: BridgeTriageImportStatus? = nil,
            metadataProvenance: BridgeMetadataProvenance? = nil,
            reading: BridgeTriageReading = .unidentified
        ) -> BridgeTriageRow {
            BridgeTriageRow(
                candidateKey: candidate.key,
                folderName: candidate.displayName,
                watchedFolderPath: importWatchedFolder.path,
                displayPath: candidate.displayName,
                actionable: true,
                placement: placement,
                actionBasis: BridgeCandidateActionBasis(
                    actionable: true,
                    placement: placement,
                    draftValid: metadataSummary != nil,
                    lookup: nil,
                    separable: false
                ),
                matched: matched,
                metadataSummary: metadataSummary,
                cover: cover,
                importStatus: importStatus,
                metadataProvenance: metadataProvenance,
                reading: reading,
                selected: false
            )
        }

        /// A preview candidate showing `identifyState`.
        static func importTabFolder(
            path: String,
            name: String,
            trackCount: UInt32 = 9,
            identifyState: IdentifyState
        ) -> Candidate {
            var candidate = Candidate(
                bridge: BridgeFolderCandidate(
                    parts: [],
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
                            "https://musicbrainz.org/release-group/group-track-mismatch",
                        albumLinksUnread: false
                    )
                ],
                yearMin: 1994,
                yearMax: 1994,
                sections: [
                    BridgePressingSection(
                        album: nil,
                        pressings: [exactPressings[0]],
                        narrowedOut: []
                    )
                ]
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
                parts: [],
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

        /// No draft yet: the row shows its folder and the folder's image.
        static let triageRowUnidentified = triageRow(
            for: importTabUnidentifiedCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: nil,
            cover: .local(path: previewArtPath("Front.png"))
        )

        /// A draft read off the folder's tags.
        static let triageRowPrefilledFromTags = triageRow(
            for: importTabTaggedCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Twelve",
                albumArtistAssignments: [artistCredit("Artist Name")]
            ),
            cover: .local(path: previewArtPath("Front.png")),
            metadataProvenance: .fileMetadata,
            reading: .prefilled
        )

        /// Both catalogs a pick paired; the draft was read from MusicBrainz.
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

        /// A pick that paired two catalogs' releases; the row names both.
        static let triageRowIdentifiedOnline = triageRow(
            for: importTabIdentifiedCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Thirteen",
                albumArtistAssignments: [artistCredit("Artist Name")]
            ),
            cover: .local(path: previewArtPath("Front.png")),
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

        /// Identified, and still asked which of several pressings it is.
        static let triageRowIdentifiedSeveralMatches = triageRow(
            for: importTabIdentifiedSeveralMatchesCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: BridgeTriageMetadataSummary(
                albumTitle: "Album Title Fourteen",
                albumArtistAssignments: [artistCredit("Artist Name")]
            ),
            cover: .local(path: previewArtPath("Front.png")),
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
        static let triageRowIdentified = triageRow(
            for: importTabCandidate,
            placement: .pending,
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: releaseDetailBridge.title,
                artist: releaseDetailBridge.artist,
                year: releaseDetailBridge.year,
                media: releaseDetailBridge.facts.media,
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
            placement: .pending,
            // Several matches leave the pressing unsettled.
            matched: BridgeMatchedRelease(
                releaseId: "rel-lead",
                title: "Album Title Five",
                artist: "Artist Name",
                pressing: nil,
                cover: nil,
                evidence: BridgeMatchEvidence(
                    source: .musicBrainz,
                    signal: nil
                )
            ),
            metadataSummary: nil,
            importStatus: nil
        )

        /// Two signals that named different releases.
        static let triageRowSeveralMatchesFromSignals = triageRow(
            for: importTabDisagreementCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowTrackMismatch = triageRow(
            for: importTabTrackMismatchCandidate,
            placement: .pending,
            matched: triageMatch(
                releaseId: "rel-track-mismatch",
                title: "Album Title Seven",
                year: 1994,
                trackCount: 10
            ),
            metadataSummary: nil
        )

        /// A release already in the library, importable like any other.
        static let triageRowAlreadyInLibrary = triageRow(
            for: importTabAlreadyInLibraryCandidate,
            placement: .pending,
            matched: triageMatch(
                releaseId: releaseDetailBridge.releaseId,
                title: "Album Title (Reissue)",
                year: 2004,
                trackCount: 14,
                signal: .barcode
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

        static let triageRowNoMatch = triageRow(
            for: importTabNoMatchCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowIdentifying = triageRow(
            for: importTabIdentifyingCandidate,
            placement: .pending,
            matched: nil,
            metadataSummary: nil
        )

        private static let importTabImportingCandidate = folderCandidates[2]
        private static let importTabDoneCandidate = folderCandidates[3]
        private static let importTabFailedCandidate = folderCandidates[4]

        /// How far the preview's running import has got.
        static let importTabImportInFlight = BridgeImportInFlight(
            progressPercent: 45,
            step: .running(phase: .measuringLoudness)
        )

        /// Pending until its running import writes the release.
        static let triageRowImporting = triageRow(
            for: importTabImportingCandidate,
            placement: .pending,
            matched: triageMatch(
                releaseId: "rel-importing",
                title: importTabImportingCandidate.displayName,
                trackCount: 15
            ),
            metadataSummary: nil
        )

        static let triageRowSkipped = triageRow(
            for: folderCandidates[1],
            placement: .skipped,
            matched: nil,
            metadataSummary: nil
        )

        static let triageRowDoneImported = triageRow(
            for: importTabDoneCandidate,
            placement: .done,
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

        /// A Done row whose release was read from a catalog.
        static let importedRowIdentified = importedRow(
            for: importTabDoneCandidate,
            title: importTabDoneCandidate.displayName,
            year: 1997,
            records: identifiedFromBothCatalogs
        )

        /// A Done row whose release was read off its files' tags.
        static let importedRowFromTags = importedRow(
            for: folderCandidates[0],
            title: "Album Title",
            year: nil,
            records: []
        )

        static func importedRow(
            for candidate: Candidate,
            title: String,
            artist: String? = "Artist Name",
            year: Int32?,
            records: [BridgeReleaseRecord]
        ) -> BridgeImportedRow {
            BridgeImportedRow(
                candidateKey: candidate.key,
                displayPath: candidate.displayName,
                actionBasis: BridgeCandidateActionBasis(
                    actionable: true,
                    placement: .done,
                    draftValid: false,
                    lookup: nil,
                    separable: false
                ),
                release: BridgeImportedReleaseSummary(
                    releaseId: "preview-release",
                    albumId: "preview-album",
                    title: title,
                    artist: artist,
                    year: year,
                    cover: nil,
                    records: records
                ),
                selected: false
            )
        }

        static let triageRowFailed = triageRow(
            for: importTabFailedCandidate,
            placement: .failed,
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
                placement: .pending,
                matched: triageMatch(
                    releaseId: releaseDetailBridge.releaseId,
                    title: releaseDetailBridge.title,
                    artist: releaseDetailBridge.artist,
                    year: releaseDetailBridge.year,
                    media: releaseDetailBridge.facts.media,
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
                placement: .pending,
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
            triageRowIdentified,
            triageRowPickAPressing,
            triageRowSeveralMatchesFromSignals,
            triageRowTrackMismatch,
            triageRowAlreadyInLibrary,
            triageRowNoMatch,
            triageRowIdentifying,
            triageRowImporting,
            triageRowFailed,
        ]

        /// The row a selected Done candidate carries; the list shows it as
        /// `importedRowIdentified`.
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
                return [importedItem(importedRowIdentified)]
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
            pendingCovers: pendingCovers(
                importTabPendingRows + triageGroupedRows
            )
        )

        /// A preview row's live state offering `actions`.
        static func triageLive(
            _ actions: [BridgeCandidateAction],
            identification: BridgeIdentificationStatus? = nil,
            importing: Bool = false
        ) -> BridgeCandidateLiveState {
            BridgeCandidateLiveState(
                identification: identification,
                importing: importing,
                actions: actions
            )
        }

        /// What a row whose lookup is stored offers over its draft: no
        /// Identify, since the stored lookup is shown as it stood.
        private static let identifiedDraftCommands: [BridgeCandidateAction] = [
            .resetToFileMetadata, .clearMetadata, .combine, .skip,
            .revealFolder,
        ]

        /// What a row no lookup is stored for offers over its draft.
        private static let unidentifiedDraftCommands: [BridgeCandidateAction] =
            [.identify] + identifiedDraftCommands

        /// Each of the tab's rows' live state, by candidate key.
        @MainActor
        private static func importTabLiveStates()
            -> [String: BridgeCandidateLiveState]
        {
            let entries: [(BridgeTriageRow, BridgeCandidateLiveState)] = [
                (
                    triageRowIdentified,
                    triageLive([.import] + identifiedDraftCommands)
                ),
                (triageRowPickAPressing, triageLive(identifiedDraftCommands)),
                (
                    triageRowSeveralMatchesFromSignals,
                    triageLive(identifiedDraftCommands)
                ),
                (triageRowTrackMismatch, triageLive(identifiedDraftCommands)),
                (
                    triageRowAlreadyInLibrary,
                    triageLive([.import] + identifiedDraftCommands)
                ),
                (triageRowNoMatch, triageLive(identifiedDraftCommands)),
                (
                    triageRowIdentifying,
                    triageLive([.skip], identification: .running)
                ),
                (triageRowImporting, triageLive([], importing: true)),
                (
                    triageRowFailed,
                    triageLive([.resetToFileMetadata, .clearMetadata])
                ),
                (
                    triageGroupedRows[0],
                    triageLive([.import] + identifiedDraftCommands)
                ),
                (
                    triageGroupedRows[1],
                    triageLive(unidentifiedDraftCommands)
                ),
                (triageRowDoneImported, triageLive([])),
                (triageRowSkipped, triageLive([.restore])),
            ]
            return Dictionary(
                entries.map { ($0.0.candidateKey, $0.1) },
                uniquingKeysWith: { first, _ in first }
            )
        }

        /// Every row the tab holds, on any tab, by candidate key.
        @MainActor
        private static func importTabRowsByKey() -> [String: BridgeTriageRow] {
            let rows =
                importTabPendingRows + triageGroupedRows + importTabDoneRows
                + [triageRowSkipped]
            return Dictionary(
                rows.map { ($0.candidateKey, $0) },
                uniquingKeysWith: { first, _ in first }
            )
        }

        /// The failed folder check the pane states for a preview candidate.
        static let importTabFolderChecks: [String: BridgeFolderCheck] = [
            importTabTrackMismatchCandidate.key: .trackCountDisagrees(
                local: 1,
                source: 10
            )
        ]

        /// Where the pane places a preview candidate, from its list row.
        static func panePlacement(
            of row: BridgeTriageRow
        ) -> BridgeCandidatePanePlacement {
            let records: [BridgeReleaseRecord] =
                if case .identified(let records) = row.reading { records }
                else { [] }
            return switch row.placement {
            case .pending:
                .pending(
                    folderCheck: importTabFolderChecks[row.candidateKey],
                    records: records
                )
            case .failed:
                .pending(folderCheck: nil, records: records)
            case .skipped: .skipped(records: records)
            case .done: .done
            }
        }

        @MainActor
        static func importTabScene() -> ImportPreviewFixture {
            let store = ImportStore()
            store.applySummary(importTabSummary)
            let rows = importTabRowsByKey()
            let live = importTabLiveStates()
            for var candidate in importTabCandidates {
                candidate.placement = rows[candidate.key]
                    .map(panePlacement(of:))
                candidate.live = live[candidate.key]
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

        @MainActor
        static func importTabImporter() -> Importer {
            let importingKey = importTabImportingCandidate.key
            let inFlight = importTabImportInFlight
            let live = importTabLiveStates()
            return Importer(
                candidateRuntime: { key in
                    guard key == importingKey else { return nil }
                    return BridgeCandidateRuntimeSnapshot(
                        identifyState: .idle,
                        import: inFlight,
                        search: nil
                    )
                },
                subscribeCandidateLiveState: { key, _, callback in
                    if let state = live[key] {
                        callback.onValue(value: state)
                    }
                    return PreviewLiveStateSubscription()
                }
            )
        }

    }

    /// A preview row's live state, delivered once as it opens.
    private final class PreviewLiveStateSubscription: LiveSubscriptionProtocol,
        @unchecked Sendable
    {
        func cancel() {}
    }
#endif
