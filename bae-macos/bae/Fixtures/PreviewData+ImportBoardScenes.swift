import BaeKit
import Foundation

@testable import bae

/// The import boards' stores and streams.
extension PreviewData {
    // MARK: - The boards' scenes

    /// What an import board lists and which folder it opens.
    enum ImportBoard {
        /// The Found tab; Northbound running its import or identified.
        case found(northbound: NorthboundStanding, opening: BoardFolder)
        /// The Imported tab, Northbound opened.
        case imported

        var tab: BridgeTriageTab {
            switch self {
            case .found: .pending
            case .imported: .done
            }
        }

        /// How Northbound stands in the Found tab.
        var northbound: NorthboundStanding {
            switch self {
            case .found(let standing, _): standing
            case .imported: .importing
            }
        }
    }

    /// The store, lists, selection and streams of one import board: the
    /// counts, identify progress and imports running as the board shows
    /// them, the opened folder's candidate in the store.
    struct ImportBoardScene {
        let fixture: ImportPreviewFixture
        let selection: ImportSelection
        let importPercents: LatestStream<UInt32>
        let outbox: OutboxStore
    }

    /// The candidate `board` opens, as the pane reads it.
    @MainActor
    static func openedCandidate(_ board: ImportBoard) -> Candidate {
        switch board {
        case .imported:
            return northboundImportedCandidate()
        case .found(let standing, let folder):
            switch folder {
            case .fieldRecordings:
                return fieldRecordingsCandidate()
            case .copperSky:
                return notFoundCandidate(
                    NotFoundFolder(
                        folder: .copperSky,
                        album: "Copper Sky",
                        artist: "Odile Fen",
                        catalogNumber: "FR-12"
                    ),
                    section: .search,
                    typed: true
                )
            case .winterKitchen:
                return notFoundCandidate(
                    NotFoundFolder(
                        folder: .winterKitchen,
                        album: "Winter Kitchen",
                        artist: "Odile Fen",
                        catalogNumber: "ODF-7"
                    ),
                    section: .automatic,
                    typed: false
                )
            case .northbound where standing == .identified:
                return northboundDraftCandidate()
            default:
                preconditionFailure("no board opens \(folder)")
            }
        }
    }

    /// The upload of Northbound's release, a little under a third done.
    @MainActor
    static func northboundUploads() -> OutboxStore {
        OutboxStore(
            uploadsPaused: false,
            releaseProgress: .releaseUploads(holding: [
                boardImported(.northbound).releaseId:
                    BridgeReleaseUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 10_900_000,
                            bytesTotal: 36_400_000
                        ),
                        throughputBps: 3_200_000
                    )
            ])
        )
    }

    @MainActor
    static func importBoardScene(_ board: ImportBoard) -> ImportBoardScene {
        let store = ImportStore()
        store.applySummary(
            importQueueSummary(
                watchedFolders: [importWatchedFolder],
                hasGroups: true
            )
        )
        store.tabCounts = tabCounts(pending: 14, done: 31, skipped: 2)
        if case .found = board {
            store.identificationProgress = (identified: 4, total: 10)
            store.importsInFlight = 2
        }
        let opened = openedCandidate(board)
        store.selectedCandidates[opened.key] = opened
        return ImportBoardScene(
            fixture: ImportPreviewFixture(
                store: store,
                itemsByTab: [
                    .pending: foundItems(northbound: board.northbound),
                    .done: doneItems(),
                    .skipped: [],
                ]
            ),
            selection: importSelection(of: [opened.key], in: store),
            importPercents: .importPercents(
                holding: [BoardFolder.northbound.key: 60]
            ),
            outbox: northboundUploads()
        )
    }
}
