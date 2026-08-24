import BaeKit
import Foundation

@testable import bae

/// The walkthrough folder the mapping-pane tests read: thirteen audio files
/// against a release that names twelve tracks, plus the CUE+FLAC shape of the
/// same release — one container a track sheet carves twelve entries out of.
///
/// Everything here is a bridge value, built the way core builds it, so the
/// tests exercise the pane's own reading of the mapping table rather than a
/// convenience shape invented for them.
enum MappingFixtures {
    static let candidateKey = "/Music/Downloads/Walkthrough"
    static let releaseId = "rel-walkthrough"
    static let source: BridgeMetadataSource = .musicBrainz
    static let pick: BridgeIdentityPick = .release(
        source: source,
        releaseId: releaseId
    )

    // MARK: - Thirteen files, twelve tracks

    static func audioFile(_ index: Int) -> BridgeMappingFile {
        BridgeMappingFile(
            fileId: "\(index).flac",
            name: "\(index).flac",
            size: UInt64(30_000_000 + index * 1_000_000),
            localPath: "/tmp/walkthrough/\(index).flac",
            probedDurationMs: UInt64(200_000 + index * 1000),
            role: .audio,
            alternatives: [.audio, .notATrack],
            roleChoice: .audio
        )
    }

    static func trackEdit(
        _ index: Int,
        title: String,
        file: BridgeAudioFile?
    ) -> BridgeRawTrackEdit {
        BridgeRawTrackEdit(
            id: "import-track-\(index)",
            title: title,
            artistText: "",
            side: 1,
            trackNumber: nil,
            file: file
        )
    }

    /// One loose audio file and the track the release puts on it.
    static func pairedRow(_ index: Int) -> BridgeMappingRow {
        .unit(
            unit: BridgeMappingUnit(
                source: .file(file: audioFile(index)),
                becomes: .track(
                    track: trackEdit(
                        index - 1,
                        title: "Track \(index)",
                        file: .standalone(fileId: "\(index).flac")
                    ),
                    sourcePosition: "\(index)",
                    sourceDurationMs: UInt64(200_000 + index * 1000)
                )
            )
        )
    }

    static let thirteenFileTable = thirteenFileTable(lastTitle: "")

    /// The same thirteen rows with the last one named — what the next read
    /// answers with once that row has been written.
    static func thirteenFileTable(lastTitle: String) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            rows: (1...12).map(pairedRow)
                + [
                    .unit(
                        unit: BridgeMappingUnit(
                            source: .file(file: audioFile(13)),
                            becomes: .track(
                                track: trackEdit(
                                    12,
                                    title: lastTitle,
                                    file: .standalone(fileId: "13.flac")
                                ),
                                sourcePosition: nil,
                                sourceDurationMs: nil
                            )
                        )
                    )
                ],
            reconciliation: .moreFiles(files: 13, tracks: 12)
        )
    }

    // MARK: - One container, before and after its sheet is bound

    static let containerId = "Album Title.flac"
    static let sheetId = "Album Title.cue"
    static let containerPath = "/tmp/walkthrough/Album Title.flac"

    static let containerFile = BridgeMappingFile(
        fileId: containerId,
        name: containerId,
        size: 380_000_000,
        localPath: containerPath,
        probedDurationMs: 2_400_000,
        role: .audio,
        alternatives: [.audio, .notATrack],
        roleChoice: .audio
    )

    static let container = BridgeMappingContainer(
        fileId: containerId,
        name: containerId,
        size: 380_000_000
    )

    static func sheetGroup(
        container: BridgeMappingContainer?,
        assignment: BridgeSheetDisc
    ) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: sheetId,
            name: sheetId,
            localPath: "/tmp/walkthrough/\(sheetId)",
            bound: container.map { .describes(container: $0) }
                ?? .unresolved(requested: [containerId]),
            assignment: assignment,
            discOptions: [1, 2]
        )
    }

    /// A track the release names that the folder has nothing for.
    static func missingRow(_ index: Int) -> BridgeMappingRow {
        .unit(
            unit: BridgeMappingUnit(
                source: .missing,
                becomes: .track(
                    track: trackEdit(
                        index,
                        title: "Track \(index + 1)",
                        file: nil
                    ),
                    sourcePosition: "\(index + 1)",
                    sourceDurationMs: UInt64(200_000 + index * 1000)
                )
            )
        )
    }

    /// The container as one loose audio file taking the release's first track,
    /// with the other eleven left with nothing behind them.
    private static func looseContainerRows(
        sheet: BridgeSheetGroup
    ) -> [BridgeMappingRow] {
        [
            .sheet(sheet: sheet, entries: []),
            .unit(
                unit: BridgeMappingUnit(
                    source: .file(file: containerFile),
                    becomes: .track(
                        track: trackEdit(
                            0,
                            title: "Track 1",
                            file: .standalone(fileId: containerId)
                        ),
                        sourcePosition: "1",
                        sourceDurationMs: 201_000
                    )
                )
            ),
        ] + (1..<12).map(missingRow)
    }

    /// The sheet describes nothing, so it carves nothing.
    static let unboundSheetTable = BridgeMappingTable(
        images: [],
        rows: looseContainerRows(
            sheet: sheetGroup(container: nil, assignment: .disc(number: 1))
        ),
        reconciliation: .moreTracks(files: 1, tracks: 12)
    )

    /// An ignored sheet speaks for nothing either, so its container is loose
    /// audio again.
    static let ignoredSheetTable = BridgeMappingTable(
        images: [],
        rows: looseContainerRows(
            sheet: sheetGroup(container: container, assignment: .ignored)
        ),
        reconciliation: .moreTracks(files: 1, tracks: 12)
    )

    /// One entry of the bound sheet, carved out of the container.
    static func entry(_ index: Int) -> BridgeMappingUnit {
        BridgeMappingUnit(
            source: .sheetEntry(
                entry: BridgeMappingEntry(
                    sheetId: sheetId,
                    index: UInt32(index),
                    number: UInt32(index + 1),
                    title: "Sheet Track \(index + 1)",
                    durationMs: UInt64(200_000 + index * 1000),
                    containerId: containerId,
                    containerName: containerId,
                    containerLocalPath: containerPath
                )
            ),
            becomes: .track(
                track: trackEdit(
                    index,
                    title: "Track \(index + 1)",
                    file: .sheetSlice(
                        fileId: containerId,
                        sheetId: sheetId,
                        index: UInt32(index)
                    )
                ),
                sourcePosition: "\(index + 1)",
                sourceDurationMs: UInt64(200_000 + index * 1000)
            )
        )
    }

    /// The same folder once the sheet is bound: twelve entries out of one file.
    static func boundSheetTable(
        assignment: BridgeSheetDisc = .disc(number: 1)
    ) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            rows: [
                .sheet(
                    sheet: sheetGroup(
                        container: container,
                        assignment: assignment
                    ),
                    entries: (0..<12).map(entry)
                )
            ],
            reconciliation: .agrees(count: 12)
        )
    }

    /// What the folder's own tags say it is: two tracks, no release behind
    /// them, so the table carries no tally.
    static let unknownTable = BridgeMappingTable(
        images: [],
        rows: (1...2)
            .map { index in
                BridgeMappingRow.unit(
                    unit: BridgeMappingUnit(
                        source: .file(file: audioFile(index)),
                        becomes: .track(
                            track: BridgeRawTrackEdit(
                                id: "unknown-track-\(index - 1)",
                                title: "Track \(index)",
                                artistText: "",
                                side: 1,
                                trackNumber: Int32(index),
                                file: .standalone(fileId: "\(index).flac")
                            ),
                            sourcePosition: "\(index)",
                            sourceDurationMs: nil
                        )
                    )
                )
            },
        reconciliation: nil
    )

    // MARK: - The album fields alongside the table

    static let albumSeed = BridgeReleaseUserEdit(
        albumTitle: "Album Title",
        albumArtistNames: ["Artist Name"],
        pressing: BridgePressingEdit(
            year: 1996,
            format: "CD",
            label: nil,
            catalogNumber: nil,
            country: nil,
            barcode: nil
        ),
        tracks: []
    )

    static let albumEdit = BridgeRawReleaseEdit(
        albumTitle: "Album Title",
        albumArtistText: "Artist Name",
        pressing: BridgeRawPressingEdit(
            year: "1996",
            format: "CD",
            label: "",
            catalogNumber: "",
            country: "",
            barcode: ""
        ),
        tracks: []
    )

    /// The release the fixture folder is picked as, as its documents describe
    /// it.
    static let releaseDetail = BridgeReleaseDetail(
        releaseId: releaseId,
        source: source,
        sourceGroupId: nil,
        title: "Album Title",
        artist: "Artist Name",
        year: 1996,
        format: "CD",
        label: nil,
        catalogNumber: nil,
        country: nil,
        barcode: nil,
        trackCount: 12,
        tracks: [],
        coverArt: [],
        defaultCover: nil
    )

    /// The pressing the fixture release states — what claiming it exactly is a
    /// claim about, and what an edit is read against.
    static let exactPressing = BridgeRawPressingEdit(
        year: "1996",
        format: "CD",
        label: "",
        catalogNumber: "",
        country: "",
        barcode: ""
    )

    static let emptyFiles = BridgeCandidateFiles(
        files: [],
        formatLabel: "FLAC",
        collapsedDirectories: []
    )

    /// The value the per-candidate read answers with for the fixture folder:
    /// picked as the release above, with `mapping` as its table.
    @MainActor
    static func detail(
        mapping: BridgeMappingTable?,
        edit: BridgeRawReleaseEdit? = albumEdit,
        picked: BridgeIdentityPick? = pick,
        failure: BridgeImportFailure? = nil
    ) -> BridgeImportCandidateDetail {
        let folder = BridgeFolderCandidate(
            folderPath: candidateKey,
            sourceFolderName: "Walkthrough",
            watchedFolderPath: "/Music/Downloads",
            files: emptyFiles,
            trackCount: 13,
            skipped: false,
            isAdded: false
        )
        return BridgeImportCandidateDetail(
            candidate: folder,
            actionable: true,
            resumedIdentifyState: .idle,
            row: BridgeTriageRow(
                candidateKey: candidateKey,
                folderName: "Walkthrough",
                watchedFolderPath: "/Music/Downloads",
                displayPath: "Walkthrough",
                resolvedBoundaries: [],
                combineAncestorKey: nil,
                actionable: true,
                placement: .ready,
                skipAction: .skip,
                matched: nil,
                selectable: true,
                importStatus: nil,
                picked: picked,
                claim: nil
            ),
            release: picked == nil ? nil : releaseDetail,
            pickedLibraryStatus: nil,
            fileEvidence: [],
            edit: picked == nil ? nil : edit,
            mapping: mapping
                ?? BridgeMappingTable(
                    images: [],
                    rows: [],
                    reconciliation: nil
                ),
            unprobed: [],
            cover: nil,
            signals: nil,
            failure: failure
        )
    }

    /// A store holding one folder candidate read as the release picked for it,
    /// with `mapping` as the table core answers with.
    @MainActor
    static func store(mapping: BridgeMappingTable?) -> ImportStore {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: candidateKey,
            detail: detail(mapping: mapping)
        )
        return store
    }

    /// The mapping table the store's one candidate holds.
    @MainActor
    static func mapping(of store: ImportStore) -> BridgeMappingTable {
        store.selectedCandidates[candidateKey]?.mapping
            ?? BridgeMappingTable(images: [], rows: [], reconciliation: nil)
    }

    /// Whether bae-core can shape what the pane would commit into a savable
    /// release — the only thing standing between the commit bar's button and an
    /// import.
    @MainActor
    static func isCommittable(_ store: ImportStore) -> Bool {
        guard let candidate = store.selectedCandidates[candidateKey],
            var edit = candidate.edit
        else { return false }
        edit.tracks = bridgeMappingTracks(table: candidate.mapping)
        if case .valid = shapeReleaseEdit(raw: edit) { return true }
        return false
    }
}
