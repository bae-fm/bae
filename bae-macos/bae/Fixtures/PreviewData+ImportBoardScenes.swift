import BaeKit
import Foundation

@testable import bae

/// The import boards' stores and streams.
extension PreviewData {
    // MARK: - The boards' scenes

    /// What an import board lists and which folder it opens.
    enum ImportBoard {
        /// The Found tab, Northbound as `northbound` has it, and `opening`
        /// opened in the pane; with none, the pane asks for a folder.
        case found(northbound: NorthboundStanding, opening: FoundPane?)
        /// The Imported tab, Northbound opened.
        case imported
        /// The Skipped tab, Harbor Waltz opened.
        case skipped

        var tab: BridgeTriageTab {
            switch self {
            case .found: .pending
            case .imported: .done
            case .skipped: .skipped
            }
        }

        /// How Northbound stands in the Found tab.
        var northbound: NorthboundStanding {
            switch self {
            case .found(let standing, _): standing
            case .imported, .skipped: .importing
            }
        }
    }

    /// The folder a Found board opens, as its pane shows it.
    enum FoundPane {
        /// Northbound on its draft, or on its running import when the list
        /// has it importing.
        case northbound
        /// Low Tide Hours, a release the library already holds.
        case lowTideHours
        /// Field Recordings and its CUE sheets.
        case fieldRecordings
        /// untitled 07, with nothing read from its files.
        case untitled07
        /// Copper Sky, which no catalog has, on Find online's Search with
        /// its typed search out.
        case copperSky
        /// Winter Kitchen, which no catalog has, on Find online's
        /// Automatic.
        case winterKitchen
    }

    /// The store, lists, selection and streams of one import board: the
    /// counts, identify progress and imports running as the board shows
    /// them, the opened folder's read in the store, and what it has in
    /// flight.
    struct ImportBoardScene {
        let fixture: ImportPreviewFixture
        let selection: ImportSelection
        let runtimes: [Data: BridgeCandidateRuntimeSnapshot]
        let importPercents: LatestStream<UInt32>
        let outbox: OutboxStore
    }

    /// What `pane` opens, as the pane reads it, with Northbound as
    /// `northbound` has it.
    @MainActor
    static func openedCandidate(
        _ pane: FoundPane,
        northbound: NorthboundStanding
    ) -> Candidate {
        switch pane {
        case .northbound:
            switch northbound {
            case .identified:
                northboundDraftCandidate()
            case .importing:
                northboundDraftCandidate(
                    importStep: .running(phase: .measuringLoudness)
                )
            }
        case .lowTideHours: lowTideHoursCandidate()
        case .fieldRecordings: fieldRecordingsCandidate()
        case .untitled07: untitledCandidate()
        case .copperSky:
            notFoundCandidate(
                copperSky,
                session: paneSession(
                    presentation: .findOnline,
                    section: .search,
                    artist: copperSky.artist,
                    album: copperSky.album
                )
            )
        case .winterKitchen:
            notFoundCandidate(
                winterKitchen,
                session: paneSession(presentation: .findOnline)
            )
        }
    }

    /// What the folder `board` opens has in flight, by its key: what
    /// identification found for it, and its typed search. The library of
    /// Winter Kitchen's board has no Discogs key, so its run asked
    /// MusicBrainz alone.
    static func boardRuntimes(
        _ board: ImportBoard
    ) -> [Data: BridgeCandidateRuntimeSnapshot] {
        switch board {
        case .found(_, .copperSky?):
            [
                BoardFolder.copperSky.key: notFoundRuntime(
                    copperSky,
                    providers: [.musicBrainz, .discogs],
                    search: copperSkySearch
                )
            ]
        case .found(_, .winterKitchen?):
            [
                BoardFolder.winterKitchen.key: notFoundRuntime(
                    winterKitchen,
                    providers: [.musicBrainz]
                )
            ]
        case .found, .imported, .skipped:
            [:]
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
                            bytesTotal: 36_400_000,
                            fraction: 0.3
                        ),
                        throughputBps: 3_200_000
                    )
            ])
        )
    }

    /// One row selected, as core's selection read answers for it: the row,
    /// and what it offers.
    @MainActor
    static func boardSelection(
        _ single: BridgeSingleSelection,
        key: Data,
        offers: [BridgeSelectionOffer]
    ) -> ImportSelection {
        let selection = ImportSelection()
        selection.select([key])
        selection.apply(
            BridgeSelectionSummary(count: 1, single: single, offers: offers)
        )
        return selection
    }

    /// Opens the folder `board` opens: its read in `store`, and the
    /// selection of its row.
    @MainActor
    static func openBoardFolder(
        _ board: ImportBoard,
        in store: ImportStore
    ) -> ImportSelection {
        switch board {
        case .found(_, nil):
            return ImportSelection()
        case .found(let northbound, let pane?):
            let opened = openedCandidate(pane, northbound: northbound)
            store.selectedCandidates[opened.key] = opened
            return boardSelection(
                .candidate(key: opened.key),
                key: opened.key,
                offers: opened.live?.offers ?? []
            )
        case .imported:
            let opened = northboundImportedCandidate()
            store.selectedCandidates[opened.key] = opened
            return boardSelection(
                .candidate(key: opened.key),
                key: opened.key,
                offers: opened.live?.offers ?? []
            )
        case .skipped:
            let opened = boardHarborWaltzDetail
            let key = opened.candidate.candidateKey
            store.selectedInvalidCandidates[key] = opened
            return boardSelection(
                .invalidCandidate(key: key),
                key: key,
                offers: []
            )
        }
    }

    @MainActor
    static func importBoardScene(_ board: ImportBoard) -> ImportBoardScene {
        let store = ImportStore()
        store.applySummary(
            importQueueSummary(hasWatchedFolders: true, hasGroups: true)
        )
        if case .found = board {
            store.identificationProgress = (identified: 4, total: 10)
            store.importsInFlight = 2
        }
        let selection = openBoardFolder(board, in: store)
        return ImportBoardScene(
            fixture: ImportPreviewFixture(
                store: store,
                itemsByTab: [
                    .pending: foundItems(northbound: board.northbound),
                    .done: doneItems(),
                    .skipped: skippedItems(),
                ],
                tabCounts: BridgeTriageTabCounts(
                    pending: 14,
                    done: 31,
                    skipped: 2
                )
            ),
            selection: selection,
            runtimes: boardRuntimes(board),
            importPercents: .importPercents(
                holding: [BoardFolder.northbound.key: 60]
            ),
            outbox: northboundUploads()
        )
    }
}
