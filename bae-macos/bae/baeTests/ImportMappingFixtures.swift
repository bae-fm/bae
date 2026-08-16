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
    static let pick = CandidatePick(
        releaseId: "rel-walkthrough",
        source: .musicBrainz,
        claim: .exact
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

    /// A thirteenth file the release's tracklist does not name: it carries
    /// audio, so it writes a track, and nobody has said what that track is.
    static let unnamedRow: BridgeMappingRow = .unit(
        unit: BridgeMappingUnit(
            source: .file(file: audioFile(13)),
            becomes: .track(
                track: trackEdit(
                    12,
                    title: "",
                    file: .standalone(fileId: "13.flac")
                ),
                sourcePosition: nil,
                sourceDurationMs: nil
            )
        )
    )

    static let thirteenFileTable = BridgeMappingTable(
        images: [],
        rows: (1...12).map(pairedRow) + [unnamedRow],
        reconciliation: .moreFiles(files: 13, tracks: 12)
    )

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

    /// A prefetch carrying `mapping`, claiming the release at `level` the way
    /// core answers a pick that carries it. The pane reads the claim, the seed
    /// and the mapping; `slots` stays in the bridge for the other desktop
    /// surface, and is empty here because nothing under test reads it.
    static func prefetch(
        mapping: BridgeMappingTable,
        level: BridgeClaimLevel = .exact
    ) -> BridgeReleasePrefetch {
        BridgeReleasePrefetch(
            detail: BridgeReleaseDetail(
                releaseId: pick.releaseId,
                source: pick.source,
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
            ),
            seed: albumSeed,
            claim: BridgeClaimLine(
                choice: level == .exact
                    ? .exact(
                        releaseId: pick.releaseId,
                        source: pick.source
                    )
                    : .approximate(
                        releaseId: pick.releaseId,
                        source: pick.source
                    ),
                level: level,
                evidence: .discIdAlone,
                release: "CD \u{00b7} 1996",
                trackCount: 12
            ),
            exactPressing: exactPressing,
            mapping: mapping
        )
    }

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

    /// A store holding one folder candidate read as the release picked for it,
    /// with `mapping` on it — the state a pick leaves behind.
    @MainActor
    static func store(mapping: BridgeMappingTable?) -> ImportStore {
        let store = ImportStore()
        var candidate = Candidate(
            bridge: BridgeFolderCandidate(
                folderPath: candidateKey,
                sourceFolderName: "Walkthrough",
                watchedFolderPath: "/Music/Downloads",
                files: emptyFiles,
                trackCount: 13,
                skipped: false,
                isAdded: false
            )
        )
        candidate.pick = pick
        candidate.identityChoice = .exact(
            releaseId: pick.releaseId,
            source: pick.source
        )
        candidate.editValues = albumEdit
        candidate.exactPressing = exactPressing
        candidate.claim =
            prefetch(
                mapping: mapping
                    ?? BridgeMappingTable(
                        images: [],
                        rows: [],
                        reconciliation: nil
                    )
            )
            .claim
        candidate.mapping = mapping
        store.folderCandidates[candidate.key] = candidate
        return store
    }

    /// The mapping table the store's one candidate holds.
    @MainActor
    static func mapping(of store: ImportStore) -> BridgeMappingTable {
        store.folderCandidates[candidateKey]?.mapping
            ?? BridgeMappingTable(images: [], rows: [], reconciliation: nil)
    }

    /// Whether bae-core can shape what the pane would commit into a savable
    /// release — the only thing standing between the commit bar's button and an
    /// import.
    @MainActor
    static func isCommittable(_ store: ImportStore) -> Bool {
        guard let edit = store.folderCandidates[candidateKey]?.commitEdit
        else { return false }
        if case .valid = shapeReleaseEdit(raw: edit) { return true }
        return false
    }
}
