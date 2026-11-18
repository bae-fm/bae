import AppKit
import BaeKit
import Foundation

@testable import bae

/// Fixtures for the import candidate list and its rows.
extension PreviewData {
    // MARK: - Triage sidebar

    /// A `BridgeTriageRow` fixture for an existing `Candidate` fixture.
    static func triageRow(
        for candidate: Candidate,
        placement: BridgeTriagePlacement,
        metadataSummary: BridgeTriageMetadataSummary? = nil,
        cover: BridgeCoverImageSource? = nil,
        importStatus: BridgeTriageImportStatus? = nil,
        upload: BridgeUploadProgress? = nil,
        metadataProvenance: BridgeMetadataProvenance? = nil,
        reading: BridgeTriageReading = .unidentified,
        live: BridgeCandidateLiveState = triageLive([])
    ) -> BridgeTriageRow {
        BridgeTriageRow(
            candidateKey: candidate.key,
            folderName: candidate.displayName,
            watchedFolderPath: importWatchedFolderPath,
            actionable: true,
            placement: placement,
            live: live,
            metadataSummary: metadataSummary,
            cover: cover,
            importStatus: importStatus,
            upload: upload,
            metadataProvenance: metadataProvenance,
            reading: reading
        )
    }

    /// A shot candidate: a folder under the import root.
    static func importTabFolder(
        path: String,
        name: String,
        trackCount: UInt32 = 9
    ) -> Candidate {
        Candidate(
            folder: BridgeFolderCandidate(
                parts: [],
                candidateKey: Data("\(importRootPath)/\(path)".utf8),
                folderPath: Data("\(importRootPath)/\(path)".utf8),
                sourceFolderName: name,
                watchedFolderPath: importWatchedFolderPath,
                files: bridgeCandidateFiles,
                trackCount: trackCount,
                skipped: false,
            ),
            folders: .one(folderPath: Data("\(importRootPath)/\(path)".utf8))
        )
    }

    static let importTabSeveralMatchesCandidate = importTabFolder(
        path: "Album Title Five",
        name: "Album Title Five",
        trackCount: 12
    )

    static let importTabDisagreementCandidate = importTabFolder(
        path: "Album Title Six - Remaster",
        name: "Album Title Six - Remaster",
        trackCount: 11
    )

    static let importTabNoTracklistCandidate = importTabFolder(
        path: "Album Title Seven",
        name: "Album Title Seven",
        trackCount: 10
    )

    static let importTabAlreadyInLibraryCandidate = importTabFolder(
        path: "Album Title Eight - Reissue",
        name: "Album Title Eight - Reissue",
        trackCount: 14
    )

    static let importTabNoMatchCandidate = importTabFolder(
        path: "Unmatched Folder",
        name: "Unmatched Folder"
    )

    static let importTabIdentifyingCandidate = importTabFolder(
        path: "Queued Folder",
        name: "Queued Folder"
    )

    @MainActor
    private static let importTabGroupedReadyCandidate = paneCandidate(
        folder: BridgeFolderCandidate(
            parts: [],
            candidateKey: Data(
                "\(importRootPath)/Artist Collection/Album Title Nine".utf8
            ),
            folderPath: Data(
                "\(importRootPath)/Artist Collection/Album Title Nine".utf8
            ),
            sourceFolderName: "Album Title Nine",
            watchedFolderPath: importWatchedFolderPath,
            files: bridgeCandidateFiles,
            trackCount: 9,
            skipped: false,
        ),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: releaseDetailBridge.source,
                key: releaseDetailBridge.releaseId
            )
        ),
        edit: confirmEditValues,
        mapping: everyRowKindMappingTable,
        cover: releaseDetailBridge.defaultCover
    )

    @MainActor
    private static let importTabGroupedCandidates = [
        importTabGroupedReadyCandidate,
        importTabFolder(
            path: "Artist Collection/Album Title Ten",
            name: "Album Title Ten"
        ),
    ]

    private static let importTabUnidentifiedCandidate = importTabFolder(
        path: "Release Folder Eleven",
        name: "Release Folder Eleven"
    )

    private static let importTabTaggedCandidate = importTabFolder(
        path: "Release Folder Twelve",
        name: "Release Folder Twelve"
    )

    private static let importTabIdentifiedCandidate = importTabFolder(
        path: "Release Folder Thirteen",
        name: "Release Folder Thirteen"
    )

    private static let importTabIdentifiedSeveralMatchesCandidate =
        importTabFolder(
            path: "Release Folder Fourteen",
            name: "Release Folder Fourteen"
        )

    /// No draft yet: the row shows its folder and the folder's image.
    static let triageRowUnidentified = triageRow(
        for: importTabUnidentifiedCandidate,
        placement: .pending,
        metadataSummary: nil,
        cover: .local(file: previewArtFile("Front.png"))
    )

    /// A draft read off the folder's tags.
    static let triageRowPrefilledFromTags = triageRow(
        for: importTabTaggedCandidate,
        placement: .pending,
        metadataSummary: BridgeTriageMetadataSummary(
            albumTitle: "Album Title Twelve",
            albumArtists: .named("Artist Name")
        ),
        cover: .local(file: previewArtFile("Front.png")),
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
        metadataSummary: BridgeTriageMetadataSummary(
            albumTitle: "Album Title Thirteen",
            albumArtists: .named("Artist Name")
        ),
        cover: .local(file: previewArtFile("Front.png")),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: .musicBrainz,
                key: "rel-paired"
            )
        ),
        reading: .identified(records: identifiedFromBothCatalogs)
    )

    /// Identified, and still asked which of several pressings it is.
    static let triageRowIdentifiedSeveralMatches = triageRow(
        for: importTabIdentifiedSeveralMatchesCandidate,
        placement: .pending,
        metadataSummary: BridgeTriageMetadataSummary(
            albumTitle: "Album Title Fourteen",
            albumArtists: .named("Artist Name")
        ),
        cover: .local(file: previewArtFile("Front.png")),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: .musicBrainz,
                key: "rel-several"
            )
        ),
        reading: .identified(records: identifiedFromBothCatalogs)
    )

    @MainActor
    static let triageRowIdentified = triageRow(
        for: importTabCandidate,
        placement: .pending,
        metadataSummary: nil,
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: releaseDetailBridge.source,
                key: releaseDetailBridge.releaseId
            )
        ),
        live: triageLive([.import] + identifiedDraftCommands)
    )

    static let triageRowPickAPressing = triageRow(
        for: importTabSeveralMatchesCandidate,
        placement: .pending,
        metadataSummary: nil,
        importStatus: nil,
        live: triageLive(identifiedDraftCommands)
    )

    /// Two signals that named different releases.
    static let triageRowSeveralMatchesFromSignals = triageRow(
        for: importTabDisagreementCandidate,
        placement: .pending,
        metadataSummary: nil,
        live: triageLive(identifiedDraftCommands)
    )

    static let triageRowNoTracklist = triageRow(
        for: importTabNoTracklistCandidate,
        placement: .pending,
        metadataSummary: nil,
        live: triageLive(identifiedDraftCommands)
    )

    /// A release already in the library, importable like any other.
    static let triageRowAlreadyInLibrary = triageRow(
        for: importTabAlreadyInLibraryCandidate,
        placement: .pending,
        metadataSummary: nil,
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: releaseDetailBridge.source,
                key: releaseDetailBridge.releaseId
            )
        ),
        live: triageLive([.import] + identifiedDraftCommands)
    )

    static let triageRowNoMatch = triageRow(
        for: importTabNoMatchCandidate,
        placement: .pending,
        metadataSummary: nil,
        live: triageLive(
            identifiedDraftCommands,
            standing: .needsYou(reason: .notFound),
            badge: BridgeRowBadge(
                says: .needsYou(reason: .notFound),
                tone: .attention
            )
        )
    )

    static let triageRowIdentifying = triageRow(
        for: importTabIdentifyingCandidate,
        placement: .pending,
        metadataSummary: nil,
        live: triageLive(
            [.skip],
            identifying: .running,
            standing: .identifying
        )
    )

    private static let importTabImportingCandidate = folderCandidates[2]
    private static let importTabDoneCandidate = folderCandidates[3]
    private static let importTabFailedCandidate = folderCandidates[4]

    /// Pending until its running import writes the release.
    static let triageRowImporting = triageRow(
        for: importTabImportingCandidate,
        placement: .pending,
        metadataSummary: nil,
        live: triageLive(
            [],
            importStep: .running(phase: .measuringLoudness)
        )
    )

    static let triageRowSkipped = triageRow(
        for: folderCandidates[1],
        placement: .skipped,
        metadataSummary: nil,
        live: triageLive([.restore])
    )

    /// A Done row: the row it was in Found, read from a catalog, its
    /// import status naming the release imported from it.
    static let triageRowDoneImported = triageRow(
        for: importTabDoneCandidate,
        placement: .done,
        metadataSummary: BridgeTriageMetadataSummary(
            albumTitle: "Album Title Sixteen",
            albumArtists: .named("Artist Name")
        ),
        cover: .local(file: previewArtFile("Front.png")),
        importStatus: .complete(
            release: BridgeImportedRelease(
                releaseId: "preview-release",
                albumId: "preview-album"
            )
        ),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: .musicBrainz,
                key: "rel-paired"
            )
        ),
        reading: .identified(records: identifiedFromBothCatalogs),
        live: triageLive([.revealFolder])
    )

    /// A Done row read off its files' tags.
    static let triageRowDoneFromTags = triageRow(
        for: folderCandidates[0],
        placement: .done,
        metadataSummary: BridgeTriageMetadataSummary(
            albumTitle: "Album Title",
            albumArtists: .named("Artist Name")
        ),
        importStatus: .complete(
            release: BridgeImportedRelease(
                releaseId: "preview-release-tags",
                albumId: "preview-album-tags"
            )
        ),
        metadataProvenance: .fileMetadata,
        reading: .prefilled,
        live: triageLive([.revealFolder])
    )

    static let triageRowFailed = triageRow(
        for: importTabFailedCandidate,
        placement: .failed,
        metadataSummary: nil,
        importStatus: .error(
            // As the import's decode check words a truncated track.
            error: .Diagnostic(
                category: .import,
                detail:
                    "decode verification failed for 1 track(s): Seventh Track (track 7): decoded 2116800 of 10584000 expected frames (truncated body)"
            )
        ),
        live: triageLive(
            [.resetToFileMetadata, .clearMetadata],
            standing: .importError,
            badge: BridgeRowBadge(says: .importError, tone: .failure)
        )
    )

    @MainActor
    private static let triageGroupedRows = [
        triageRow(
            for: importTabGroupedReadyCandidate,
            placement: .pending,
            metadataSummary: nil,
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: releaseDetailBridge.source,
                    key: releaseDetailBridge.releaseId
                )
            ),
            live: triageLive([.import] + identifiedDraftCommands)
        ),
        triageRow(
            for: importTabGroupedCandidates[1],
            placement: .pending,
            metadataSummary: nil,
            live: triageLive(unidentifiedDraftCommands)
        ),
    ]

    @MainActor
    static let importTabCandidates =
        [
            importTabCandidate,
            importTabSeveralMatchesCandidate,
            importTabDisagreementCandidate,
            importTabNoTracklistCandidate,
            importTabAlreadyInLibraryCandidate,
            importTabNoMatchCandidate,
            importTabIdentifyingCandidate,
            importTabImportingCandidate,
            importTabDoneCandidate,
            importTabFailedCandidate,
            folderCandidates[1],
        ] + importTabGroupedCandidates

    private static let importTabGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: importWatchedFolderPath,
        relativeFolder: Data("Artist Collection".utf8)
    )

    @MainActor
    private static let importTabPendingRows = [
        triageRowIdentified,
        triageRowPickAPressing,
        triageRowSeveralMatchesFromSignals,
        triageRowNoTracklist,
        triageRowAlreadyInLibrary,
        triageRowNoMatch,
        triageRowIdentifying,
        triageRowImporting,
        triageRowFailed,
    ]

    /// The rows the Done tab shows: the ones they were in Found.
    private static let importTabDoneRows = [
        triageRowDoneImported
    ]

    @MainActor
    static func importTabItems(
        _ tab: BridgeTriageTab
    ) -> [BridgeImportListItem] {
        switch tab {
        case .pending:
            let header = groupHeader(
                key: importTabGroupKey,
                name: "Artist Collection",
                entryCount: UInt32(triageGroupedRows.count)
            )
            return importTabPendingRows.map(candidateItem)
                + [.groupHeader(header: header)]
                + triageGroupedRows.map {
                    candidateItem(
                        $0,
                        header: header,
                        last: $0.candidateKey
                            == triageGroupedRows.last?.candidateKey
                    )
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
        hasWatchedFolders: true,
        hasGroups: true
    )

    /// A shot row's live state whose row offers each of `actions` for
    /// itself.
    static func triageLive(
        _ actions: [BridgeCandidateAction],
        identifying: BridgeIdentifyingStep? = nil,
        importStep: BridgeImportStep? = nil,
        standing: BridgePendingStanding? = nil,
        badge: BridgeRowBadge? = nil
    ) -> BridgeCandidateLiveState {
        BridgeCandidateLiveState(
            identifying: identifying,
            import: importStep,
            actions: actions,
            standing: standing,
            badge: badge
        )
    }

    /// What a row whose lookup is stored offers over its draft: no
    /// Identify, since the stored lookup is shown as it stood.
    private static let identifiedDraftCommands: [BridgeCandidateAction] = [
        .resetToFileMetadata, .clearMetadata, .skip, .revealFolder,
    ]

    /// What a row no lookup is stored for offers over its draft.
    private static let unidentifiedDraftCommands: [BridgeCandidateAction] =
        [.identify] + identifiedDraftCommands

    /// Every row the tab holds, on any tab, by candidate key.
    @MainActor
    private static func importTabRowsByKey() -> [Data: BridgeTriageRow] {
        let rows =
            importTabPendingRows + triageGroupedRows + importTabDoneRows
            + [triageRowSkipped]
        return Dictionary(
            rows.map { ($0.candidateKey, $0) },
            uniquingKeysWith: { first, _ in first }
        )
    }

    /// The failed folder check the pane states for a shot candidate.
    static let importTabFolderChecks: [Data: BridgeFolderCheck] = [
        importTabNoTracklistCandidate.key: .sourceTracksUnknown
    ]

    /// Where the pane places a shot candidate, from its list row.
    static func panePlacement(
        of row: BridgeTriageRow
    ) -> BridgeCandidatePanePlacement {
        let records: [BridgeReleaseRecord] =
            switch row.reading {
            case .identified(let records), .identifiedAlbum(let records):
                records
            case .prefilled, .unidentified: []
            }
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

    /// How many entries each tab of the Import tab shot holds.
    static let importTabCounts = (
        pending: UInt32(11),
        done: UInt32(1),
        skipped: UInt32(1 + invalidCandidates.count)
    )

    @MainActor
    static func importTabScene() -> ImportPreviewFixture {
        let store = ImportStore()
        store.applySummary(importTabSummary)
        let rows = importTabRowsByKey()
        for var candidate in importTabCandidates {
            candidate.placement = rows[candidate.key]
                .map(panePlacement(of:))
            candidate.live = rows[candidate.key]?.live
            store.selectedCandidates[candidate.key] = candidate
        }
        store.identificationProgress = (identified: 112, total: 130)
        return ImportPreviewFixture(
            store: store,
            itemsByTab: [
                .pending: importTabItems(.pending),
                .done: importTabItems(.done),
                .skipped: importTabItems(.skipped),
            ],
            tabCounts: BridgeTriageTabCounts(
                pending: importTabCounts.pending,
                done: importTabCounts.done,
                skipped: importTabCounts.skipped
            )
        )
    }
}

extension BridgeCandidateLiveState {
    /// A fixture row's live state whose row offers each of `actions` for
    /// itself, as core offers a row's own commands: for that one row, each
    /// enabled.
    init(
        identifying: BridgeIdentifyingStep?,
        import: BridgeImportStep?,
        actions: [BridgeCandidateAction],
        standing: BridgePendingStanding?,
        badge: BridgeRowBadge?
    ) {
        self.init(
            identifying: identifying,
            import: `import`,
            offers: actions.map {
                BridgeSelectionOffer(action: $0, count: 1, enabled: true)
            },
            standing: standing,
            badge: badge
        )
    }
}
