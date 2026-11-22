import BaeKit
import Foundation

@testable import bae

/// The import boards' sidebar rows and lists.
extension PreviewData {
    // MARK: - Rows

    /// The actions a row with a draft offers.
    static let boardDraftActions: [BridgeCandidateAction] = [
        .import, .resetToFileMetadata, .clearMetadata, .combine, .skip,
        .revealFolder,
    ]

    /// `folder`'s row, with the draft `summary` sums up when there is one,
    /// read from the MusicBrainz release `record` when there is one, else from
    /// the files' tags.
    static func boardRow(
        _ folder: BoardFolder,
        summary: BridgeTriageMetadataSummary? = nil,
        record: String? = nil,
        placement: BridgeTriagePlacement = .pending,
        live: BridgeCandidateLiveState = triageLive(boardDraftActions),
        importStatus: BridgeTriageImportStatus? = nil,
        upload: BridgeUploadProgress? = nil
    ) throws -> BridgeTriageRow {
        let cover: BridgeCoverImageSource? =
            switch folder {
            case .fieldRecordings, .fieldRecordingsDisc2:
                try fieldRecordingsCatalogCover().image
            default:
                try summary.map {
                    .local(
                        file: try BoardArtwork.scannedFile(named: $0.albumTitle)
                    )
                }
            }
        return triageRow(
            for: boardCandidate(folder),
            placement: placement,
            metadataSummary: summary,
            cover: cover,
            importStatus: importStatus,
            upload: upload,
            metadataProvenance: record.map {
                .externalRelease(
                    record: BridgeLookupRef(catalog: .musicBrainz, key: $0)
                )
            } ?? (summary == nil ? nil : .fileMetadata),
            reading: record.map { .identified(records: boardRecords($0)) }
                ?? (summary == nil ? .unidentified : .prefilled),
            live: live
        )
    }

    /// A row the lookups left to the person, for `reason`.
    static func needsYou(
        _ reason: BridgeNeedsYouReason
    ) -> BridgeCandidateLiveState {
        triageLive(
            [
                .identify, .resetToFileMetadata, .clearMetadata, .skip,
                .revealFolder,
            ],
            standing: .needsYou(reason: reason),
            badge: BridgeRowBadge(
                says: .needsYou(reason: reason),
                tone: .attention
            )
        )
    }

    /// Northbound's row while its import runs, measuring loudness.
    static let northboundImportingLive = triageLive(
        [],
        importStep: .running(phase: .measuringLoudness)
    )

    /// How Northbound stands in the Found tab: running its import, or
    /// identified and waiting.
    enum NorthboundStanding {
        case importing
        case identified
    }

    static let fieldRecordingsGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: importWatchedFolderPath,
        relativeFolder: Data(BoardFolder.fieldRecordingsGroup.utf8)
    )

    /// The Found tab's rows as the boards list them.
    static func foundItems(
        northbound: NorthboundStanding
    ) throws -> [BridgeImportListItem] {
        try groupItems() + lookupItems() + importItems(northbound: northbound)
    }

    /// Field Recordings' two discs, listed under their folder's header.
    static func groupItems() throws -> [BridgeImportListItem] {
        let header = groupHeader(
            key: fieldRecordingsGroupKey,
            name: "Mara Quill \u{2014} Field Recordings (1979)",
            entryCount: 2
        )
        return [
            .groupHeader(header: header),
            candidateItem(
                try boardRow(
                    .fieldRecordings,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Field Recordings",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-field-recordings"
                ),
                header: header
            ),
            candidateItem(
                try boardRow(
                    .fieldRecordingsDisc2,
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .running,
                        standing: .identifying
                    )
                ),
                header: header,
                last: true
            ),
        ]
    }

    /// The rows the lookups left to the person or have not reached.
    static func lookupItems() throws -> [BridgeImportListItem] {
        [
            candidateItem(
                try boardRow(
                    .lowTideHours,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Low Tide Hours",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-low-tide-hours",
                    live: needsYou(.matches(count: 3))
                )
            ),
            candidateItem(
                try boardRow(
                    .copperSky,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Copper Sky",
                        albumArtists: .named("Odile Fen")
                    ),
                    live: needsYou(.notFound)
                )
            ),
            candidateItem(
                try boardRow(.untitled07, live: needsYou(.nothingToLookUp))
            ),
            candidateItem(
                try boardRow(
                    .smallHours,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Small Hours",
                        albumArtists: .named("Tomas Brell")
                    ),
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .queued,
                        standing: .identifying
                    )
                )
            ),
            candidateItem(
                try boardRow(
                    .winterKitchen,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Winter Kitchen",
                        albumArtists: .named("Odile Fen")
                    ),
                    live: needsYou(.notFound)
                )
            ),
        ]
    }

    /// The rows importing, queued to import or failed, and Northbound as
    /// `northbound` has it.
    static func importItems(
        northbound: NorthboundStanding
    ) throws -> [BridgeImportListItem] {
        let northboundRow: BridgeTriageRow =
            switch northbound {
            case .importing:
                try boardRow(
                    .northbound,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Northbound",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-northbound",
                    live: northboundImportingLive
                )
            case .identified:
                try boardRow(
                    .northbound,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Northbound",
                        albumArtists: .named("Mara Quill")
                    ),
                    record: "board-northbound"
                )
            }
        return [
            candidateItem(northboundRow),
            candidateItem(
                try boardRow(
                    .saltAndCedar,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Salt and Cedar",
                        albumArtists: .named("The Slow Orchards")
                    ),
                    record: "board-salt-and-cedar",
                    live: triageLive([], importStep: .preparing(step: .queued))
                )
            ),
            candidateItem(
                try boardRow(
                    .paperLanterns,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Paper Lanterns",
                        albumArtists: .named("The Slow Orchards")
                    ),
                    placement: .failed,
                    live: triageLive(
                        [.resetToFileMetadata, .clearMetadata],
                        standing: .importError,
                        badge: BridgeRowBadge(
                            says: .importError,
                            tone: .failure
                        )
                    ),
                    importStatus: .error(
                        error: .Diagnostic(
                            category: .import,
                            detail: "Could not read 03 Second Floor.flac"
                        )
                    )
                )
            ),
        ]
    }

    /// The release a Done row's folder became.
    static func boardImported(_ folder: BoardFolder)
        -> BridgeImportedRelease
    {
        BridgeImportedRelease(
            releaseId: "board-release-\(folder.path)",
            albumId: "board-album-\(folder.path)"
        )
    }

    /// The Imported tab's rows as the boards list them.
    static func doneItems() throws -> [BridgeImportListItem] {
        func done(
            _ folder: BoardFolder,
            _ title: String,
            _ artist: String,
            upload: BridgeUploadProgress? = nil
        ) throws -> BridgeImportListItem {
            candidateItem(
                try boardRow(
                    folder,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: title,
                        albumArtists: .named(artist)
                    ),
                    record: "board-\(title)",
                    placement: .done,
                    live: triageLive([.revealFolder]),
                    importStatus: .complete(release: boardImported(folder)),
                    upload: upload
                )
            )
        }
        return [
            try done(
                .northbound,
                "Northbound",
                "Mara Quill",
                upload: BridgeUploadProgress(
                    queued: 0,
                    uploading: 1,
                    activity: .uploading
                )
            ),
            try done(.driftwood, "Driftwood", "Mara Quill"),
            try done(.glassHarbor, "Glass Harbor", "Ines Varga Trio"),
            try done(.lateArrivals, "Late Arrivals", "The Slow Orchards"),
        ]
    }

    // MARK: - Skipped

    /// The folder with no audio in it, which names only its folder.
    static let boardEmptyFolder = BridgeInvalidCandidate(
        candidateKey: BoardFolder.emptyFolder.key,
        folderPath: BoardFolder.emptyFolder.key,
        sourceFolderName: BoardFolder.emptyFolder.name,
        watchedFolderPath: importWatchedFolderPath,
        separable: false,
        reason: .noValidAudio,
        tags: nil,
        cover: nil
    )

    /// The folder whose tags name a release, two of its audio files
    /// damaged.
    static func boardHarborWaltz(cover: BridgeFileVersion)
        -> BridgeInvalidCandidate
    {
        BridgeInvalidCandidate(
            candidateKey: BoardFolder.harborWaltz.key,
            folderPath: BoardFolder.harborWaltz.key,
            sourceFolderName: BoardFolder.harborWaltz.name,
            watchedFolderPath: importWatchedFolderPath,
            separable: false,
            reason: .damagedAudio(count: 2),
            tags: BridgeSkippedTags(
                albumTitle: "Harbor Waltz",
                artist: .named("Ada Lindqvist")
            ),
            cover: .local(file: cover)
        )

    }

    /// The Skipped tab's rows as the board lists them.
    static func skippedItems() throws -> [BridgeImportListItem] {
        [
            boardEmptyFolder,
            boardHarborWaltz(
                cover: try BoardArtwork.scannedFile(named: "Glass Harbor")
            ),
        ]
        .map(invalidItem)
    }
}
