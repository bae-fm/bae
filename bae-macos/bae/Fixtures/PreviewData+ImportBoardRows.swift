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

    static func boardRow(
        _ folder: BoardFolder,
        title: String? = nil,
        artist: String? = nil,
        record: String? = nil,
        placement: BridgeTriagePlacement = .pending,
        live: BridgeCandidateLiveState = triageLive(boardDraftActions),
        importStatus: BridgeTriageImportStatus? = nil,
        upload: BridgeUploadProgress? = nil
    ) -> BridgeTriageRow {
        let summary = title.map {
            BridgeTriageMetadataSummary(
                albumTitle: $0,
                albumArtists: .named(artist ?? "")
            )
        }
        return triageRow(
            for: Candidate(bridge: boardFolder(folder)),
            placement: placement,
            metadataSummary: summary,
            cover: title.map { .local(file: previewArtFile($0)) },
            importStatus: importStatus,
            upload: upload,
            metadataProvenance: record.map {
                .externalRelease(
                    record: BridgeLookupRef(catalog: .musicBrainz, key: $0)
                )
            } ?? (title == nil ? nil : .fileMetadata),
            reading: record.map { .identified(records: boardRecords($0)) }
                ?? (title == nil ? .unidentified : .prefilled),
            live: live
        )
    }

    static func needsYou(
        _ reason: BridgeNeedsYouReason,
        tone: BridgeBadgeTone = .attention
    ) -> BridgeCandidateLiveState {
        triageLive(
            [
                .identify, .resetToFileMetadata, .clearMetadata, .skip,
                .revealFolder,
            ],
            standing: .needsYou(reason: reason),
            badge: BridgeRowBadge(says: .needsYou(reason: reason), tone: tone)
        )
    }

    /// How Northbound stands in the Found tab: running its import, or
    /// identified and waiting.
    enum NorthboundStanding {
        case importing
        case identified
    }

    static let fieldRecordingsGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: importWatchedFolder.path,
        relativeFolder: Data(BoardFolder.fieldRecordingsGroup.utf8)
    )

    /// The Found tab's rows as the boards list them.
    static func foundItems(
        northbound: NorthboundStanding
    ) -> [BridgeImportListItem] {
        groupItems() + lookupItems() + importItems(northbound: northbound)
    }

    /// Field Recordings' two discs, listed under their folder's header.
    static func groupItems() -> [BridgeImportListItem] {
        let header = groupHeaderStableKey(fieldRecordingsGroupKey)
        return [
            groupHeaderItem(
                key: fieldRecordingsGroupKey,
                name: "Mara Quill \u{2014} Field Recordings (1979)",
                entryCount: 2
            ),
            candidateItem(
                boardRow(
                    .fieldRecordings,
                    title: "Field Recordings",
                    artist: "Mara Quill",
                    record: "board-field-recordings"
                ),
                headerKey: header
            ),
            candidateItem(
                boardRow(
                    .fieldRecordingsDisc2,
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .running,
                        standing: .identifying
                    )
                ),
                headerKey: header
            ),
        ]
    }

    /// The rows the lookups left to the person or have not reached.
    static func lookupItems() -> [BridgeImportListItem] {
        [
            candidateItem(
                boardRow(
                    .lowTideHours,
                    title: "Low Tide Hours",
                    artist: "Mara Quill",
                    record: "board-low-tide-hours",
                    live: needsYou(.matches(count: 3))
                )
            ),
            candidateItem(
                boardRow(
                    .copperSky,
                    title: "Copper Sky",
                    artist: "Odile Fen",
                    live: needsYou(.notFound)
                )
            ),
            candidateItem(
                boardRow(.untitled07, live: needsYou(.nothingToLookUp))
            ),
            candidateItem(
                boardRow(
                    .smallHours,
                    title: "Small Hours",
                    artist: "Tomas Brell",
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .queued,
                        standing: .identifying
                    )
                )
            ),
            candidateItem(
                boardRow(
                    .winterKitchen,
                    title: "Winter Kitchen",
                    artist: "Odile Fen",
                    live: needsYou(.notFound)
                )
            ),
        ]
    }

    /// The rows importing, queued to import or failed, and Northbound as
    /// `northbound` has it.
    static func importItems(
        northbound: NorthboundStanding
    ) -> [BridgeImportListItem] {
        let northboundRow: BridgeTriageRow =
            switch northbound {
            case .importing:
                boardRow(
                    .northbound,
                    title: "Northbound",
                    artist: "Mara Quill",
                    record: "board-northbound",
                    live: triageLive(
                        [],
                        importStep: .running(phase: .measuringLoudness)
                    )
                )
            case .identified:
                boardRow(
                    .northbound,
                    title: "Northbound",
                    artist: "Mara Quill",
                    record: "board-northbound"
                )
            }
        return [
            candidateItem(northboundRow),
            candidateItem(
                boardRow(
                    .saltAndCedar,
                    title: "Salt and Cedar",
                    artist: "The Slow Orchards",
                    record: "board-salt-and-cedar",
                    live: triageLive([], importStep: .preparing(step: .queued))
                )
            ),
            candidateItem(
                boardRow(
                    .paperLanterns,
                    title: "Paper Lanterns",
                    artist: "The Slow Orchards",
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
    static func doneItems() -> [BridgeImportListItem] {
        func done(
            _ folder: BoardFolder,
            _ title: String,
            _ artist: String,
            upload: BridgeUploadProgress? = nil
        ) -> BridgeImportListItem {
            candidateItem(
                boardRow(
                    folder,
                    title: title,
                    artist: artist,
                    record: "board-\(title)",
                    placement: .done,
                    live: triageLive([.revealFolder]),
                    importStatus: .complete(release: boardImported(folder)),
                    upload: upload
                )
            )
        }
        return [
            done(
                .northbound,
                "Northbound",
                "Mara Quill",
                upload: BridgeUploadProgress(
                    queued: 0,
                    uploading: 1,
                    activity: .uploading
                )
            ),
            done(.driftwood, "Driftwood", "Mara Quill"),
            done(.glassHarbor, "Glass Harbor", "Ines Varga Trio"),
            done(.lateArrivals, "Late Arrivals", "The Slow Orchards"),
        ]
    }
}
