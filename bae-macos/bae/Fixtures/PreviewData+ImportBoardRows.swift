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
        summary: BridgeAlbumLine? = nil,
        record: String? = nil,
        cover: BridgeCoverImageSource?,
        placement: BridgeTriagePlacement = .pending,
        live: BridgeTriageListLiveState = triageLive(boardDraftActions),
        importStatus: BridgeTriageImportStatus? = nil
    ) -> BridgeTriageRow {
        triageRow(
            for: boardCandidate(folder),
            placement: placement,
            metadataSummary: summary,
            cover: cover,
            importStatus: importStatus,
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
    ) -> BridgeTriageListLiveState {
        triageLive(
            [
                .identify, .resetToFileMetadata, .clearMetadata, .skip,
                .revealFolder,
            ],
            badge: BridgeRowBadge(
                says: .needsYou(reason: reason),
                tone: .attention
            )
        )
    }

    /// How Northbound stands in the Found tab: running its import, or
    /// identified and waiting.
    enum NorthboundStanding {
        case importing
        case identified

        private static let importingStep = BridgeImportStep.running(
            phase: .measuringLoudness
        )

        var live: BridgeTriageListLiveState {
            switch self {
            case .importing:
                PreviewData.triageLive(
                    [],
                    status: .line(line: .importing(step: Self.importingStep))
                )
            case .identified:
                PreviewData.triageLive(PreviewData.boardDraftActions)
            }
        }
    }

    static let fieldRecordingsGroupKey = BridgeFolderReleaseDecisionKey(
        watchedFolderPath: importWatchedFolderPath,
        relativeFolder: Data(BoardFolder.fieldRecordingsGroup.utf8)
    )

    /// The Found tab's rows as the boards list them.
    static func foundItems(
        northbound: NorthboundStanding,
        images: FoundBoardImages
    ) -> [BridgeImportListItem] {
        groupItems(cover: images.fieldRecordings.image)
            + lookupItems(images: images.lookup)
            + importItems(northbound: northbound, images: images)
    }

    /// Field Recordings' two discs, listed under their folder's header.
    static func groupItems(cover: BridgeCoverImageSource)
        -> [BridgeImportListItem]
    {
        let header = groupHeader(
            key: fieldRecordingsGroupKey,
            name: "Mara Quill \u{2014} Field Recordings (1979)",
            entryCount: 2
        )
        return [
            .groupHeader(header: header),
            candidateItem(
                boardRow(
                    .fieldRecordings,
                    summary: BridgeAlbumLine.titleAndArtists(
                        title: "Field Recordings",
                        artistLine: .named("Mara Quill")
                    ),
                    record: "board-field-recordings",
                    cover: cover
                ),
                header: header
            ),
            candidateItem(
                boardRow(
                    .fieldRecordingsDisc2,
                    cover: nil,
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .running
                    )
                ),
                header: header,
                last: true
            ),
        ]
    }

    /// The rows the lookups left to the person or have not reached.
    static func lookupItems(images: LookupBoardImages) -> [BridgeImportListItem]
    {
        [
            candidateItem(
                lowTideHoursBoardRow(cover: images.lowTideHours.file)
            ),
            candidateItem(
                notFoundBoardRow(copperSky, cover: images.copperSky.file)
            ),
            candidateItem(
                boardRow(
                    .untitled07,
                    cover: nil,
                    live: needsYou(.nothingToLookUp)
                )
            ),
            candidateItem(
                boardRow(
                    .smallHours,
                    summary: BridgeAlbumLine.titleAndArtists(
                        title: "Small Hours",
                        artistLine: .named("Tomas Brell")
                    ),
                    cover: .local(file: images.smallHours),
                    live: triageLive(
                        [.skip, .revealFolder],
                        identifying: .queued
                    )
                )
            ),
            candidateItem(
                notFoundBoardRow(
                    winterKitchen,
                    cover: images.winterKitchen.file
                )
            ),
        ]
    }

    static func lowTideHoursBoardRow(cover: BridgeFileVersion)
        -> BridgeTriageRow
    {
        boardRow(
            .lowTideHours,
            summary: BridgeAlbumLine.titleAndArtists(
                title: "Low Tide Hours",
                artistLine: .named("Mara Quill")
            ),
            record: "board-low-tide-hours",
            cover: .local(file: cover),
            live: needsYou(.matches(count: 3))
        )
    }

    static func notFoundBoardRow(
        _ folder: NotFoundFolder,
        cover: BridgeFileVersion
    ) -> BridgeTriageRow {
        boardRow(
            folder.folder,
            summary: BridgeAlbumLine.titleAndArtists(
                title: folder.album,
                artistLine: .named(folder.artist)
            ),
            cover: .local(file: cover),
            live: needsYou(.notFound)
        )
    }

    /// Northbound and the other rows whose imports are queued or failed.
    static func importItems(
        northbound: NorthboundStanding,
        images: FoundBoardImages
    ) -> [BridgeImportListItem] {
        let northboundRow = boardRow(
            .northbound,
            summary: northboundSummary,
            record: "board-northbound",
            cover: .local(file: images.northbound.file),
            live: northbound.live
        )
        return [
            candidateItem(northboundRow),
            candidateItem(
                boardRow(
                    .saltAndCedar,
                    summary: BridgeAlbumLine.titleAndArtists(
                        title: "Salt and Cedar",
                        artistLine: .named("The Slow Orchards")
                    ),
                    record: "board-salt-and-cedar",
                    cover: .local(file: images.saltAndCedar),
                    live: triageLive([], status: .queuedToImport)
                )
            ),
            candidateItem(paperLanternsFailedRow(cover: images.paperLanterns)),
        ]
    }

    private static func paperLanternsFailedRow(cover: BridgeFileVersion)
        -> BridgeTriageRow
    {
        boardRow(
            .paperLanterns,
            summary: BridgeAlbumLine.titleAndArtists(
                title: "Paper Lanterns",
                artistLine: .named("The Slow Orchards")
            ),
            cover: .local(file: cover),
            placement: .failed,
            live: triageLive(
                [.resetToFileMetadata, .clearMetadata],
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

    /// An Imported row, whose cover and upload belong to its library release.
    static func importedBoardItem(
        _ folder: BoardFolder,
        summary: BridgeAlbumLine,
        cover: BridgeFileVersion,
        stateLine: BridgeTriageStateLine? = nil
    ) -> BridgeImportListItem {
        candidateItem(
            boardRow(
                folder,
                summary: summary,
                record: "board-\(summary.title ?? folder.name)",
                cover: .local(file: cover),
                placement: .done,
                live: triageLive(
                    [.revealFolder],
                    status: stateLine.map { .line(line: $0) }
                ),
                importStatus: .complete(release: boardImported(folder))
            )
        )
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
}
