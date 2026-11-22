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
        /// The Found tab with Northbound importing, and five other folders
        /// selected together.
        case several
        /// The Imported tab, Northbound opened.
        case imported
        /// The Skipped tab, Harbor Waltz opened.
        case skipped

        var tab: BridgeTriageTab {
            switch self {
            case .found, .several: .pending
            case .imported: .done
            case .skipped: .skipped
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
        northbound: NorthboundStanding,
        fieldRecordingsCover: BridgeCoverChoice,
        lowTideHoursCover: BridgeMappingImage
    ) throws -> Candidate {
        switch pane {
        case .northbound:
            switch northbound {
            case .identified:
                try northboundDraftCandidate()
            case .importing:
                try northboundDraftCandidate(
                    importStep: .running(phase: .measuringLoudness)
                )
            }
        case .lowTideHours: lowTideHoursCandidate(cover: lowTideHoursCover)
        case .fieldRecordings:
            fieldRecordingsCandidate(cover: fieldRecordingsCover)
        case .untitled07: untitledCandidate()
        case .copperSky:
            try notFoundCandidate(
                copperSky,
                session: paneSession(
                    presentation: .findOnline,
                    section: .search,
                    artist: copperSky.artist,
                    album: copperSky.album
                )
            )
        case .winterKitchen:
            try notFoundCandidate(
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
        case .found, .several, .imported, .skipped:
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

    /// What a selection of five folders offers, two of them ready to
    /// import: every action the card lists, with how many it applies to.
    static let fiveFolderSelectionSummary = BridgeSelectionSummary(
        count: 5,
        single: nil,
        offers: [
            (.import, 2), (.cancelImport, 0), (.combine, 0), (.separate, 0),
            (.identify, 5), (.cancelIdentification, 0),
            (.retryIdentification, 1), (.resetToFileMetadata, 5),
            (.clearMetadata, 5), (.skip, 5), (.restore, 0), (.revealFolder, 5),
        ]
        .map { action, count in
            BridgeSelectionOffer(
                action: action,
                count: count,
                enabled: count > 0
            )
        }
    )

    /// The five folders the several-selected board has chosen, and what the
    /// selection offers: two to import, one identification to retry, and the
    /// rest every folder can be told.
    @MainActor
    static var boardSeveralSelection: ImportSelection {
        let selection = ImportSelection()
        selection.select(
            Set(
                [
                    BoardFolder.lowTideHours, .copperSky, .smallHours,
                    .winterKitchen, .saltAndCedar,
                ]
                .map(\.key)
            )
        )
        selection.apply(fiveFolderSelectionSummary)
        return selection
    }

    /// Make a candidate's detail and selection available together.
    @MainActor
    static func selectBoardCandidate(
        _ candidate: Candidate,
        in store: ImportStore
    ) -> ImportSelection {
        store.selectedCandidates[candidate.key] = candidate
        return boardSelection(
            .candidate(key: candidate.key),
            key: candidate.key,
            offers: candidate.live?.offers ?? []
        )
    }

    @MainActor
    static func importBoardScene(_ board: ImportBoard) throws
        -> ImportBoardScene
    {
        let store = ImportStore()
        store.applySummary(
            importQueueSummary(hasWatchedFolders: true, hasGroups: true)
        )
        let items: [BridgeImportListItem]
        let selection: ImportSelection
        switch board {
        case .found(let northbound, let pane):
            store.identificationProgress = (identified: 4, total: 10)
            store.importsInFlight = 2
            let cover = try fieldRecordingsCatalogCover()
            let lowTideCover = try lowTideHoursCover()
            items = try foundItems(
                northbound: northbound,
                fieldRecordingsCover: cover.image,
                lowTideHoursCover: lowTideCover.file
            )
            if let pane {
                let candidate = try openedCandidate(
                    pane,
                    northbound: northbound,
                    fieldRecordingsCover: cover,
                    lowTideHoursCover: lowTideCover
                )
                selection = selectBoardCandidate(candidate, in: store)
            }
            else {
                selection = ImportSelection()
            }
        case .several:
            store.identificationProgress = (identified: 4, total: 10)
            items = try foundItems(
                northbound: .importing,
                fieldRecordingsCover: fieldRecordingsCatalogCover().image,
                lowTideHoursCover: lowTideHoursCover().file
            )
            selection = boardSeveralSelection
        case .imported:
            items = try doneItems()
            selection = selectBoardCandidate(
                try northboundImportedCandidate(),
                in: store
            )
        case .skipped:
            (items, selection) = try skippedBoard(in: store)
        }
        return makeImportBoardScene(
            board,
            store: store,
            items: items,
            selection: selection
        )
    }

    @MainActor
    private static func skippedBoard(in store: ImportStore) throws
        -> ([BridgeImportListItem], ImportSelection)
    {
        let image = try boardImage("folder.jpg", art: "Glass Harbor")
        let candidate = boardHarborWaltz(cover: image.file)
        let key = candidate.candidateKey
        store.selectedInvalidCandidates[key] = boardHarborWaltzDetail(
            candidate: candidate,
            image: image
        )
        return (
            [boardEmptyFolder, candidate].map(invalidItem),
            boardSelection(.invalidCandidate(key: key), key: key, offers: [])
        )
    }

    @MainActor
    private static func makeImportBoardScene(
        _ board: ImportBoard,
        store: ImportStore,
        items: [BridgeImportListItem],
        selection: ImportSelection
    ) -> ImportBoardScene {
        ImportBoardScene(
            fixture: ImportPreviewFixture(
                store: store,
                itemsByTab: [board.tab: items],
                tabCounts: BridgeTriageTabCounts(
                    pending: 10,
                    done: 4,
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
