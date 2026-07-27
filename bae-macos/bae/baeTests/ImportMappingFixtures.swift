import BaeKit
import Foundation

@testable import bae

/// The walkthrough folder the mapping-pane tests read: thirteen audio files
/// against a source that names twelve tracks, plus the CUE+FLAC shape of the
/// same release — one container a track sheet carves twelve tracks out of.
///
/// Everything here is a bridge value, built the way core builds it, so the
/// tests exercise the pane's own reading of the mapping rather than a
/// convenience shape invented for them.
enum MappingFixtures {
    static let candidateKey = "/Music/Downloads/Walkthrough"

    // MARK: - Thirteen files, twelve tracks

    static func standalone(_ index: Int) -> BridgeSlotFile {
        BridgeSlotFile(
            audio: .standalone(fileId: "\(index).flac"),
            name: "\(index).flac",
            size: UInt64(30_000_000 + index * 1_000_000),
            localPath: "/tmp/walkthrough/\(index).flac",
            probedDurationMs: UInt64(200_000 + index * 1000),
            span: .whole
        )
    }

    static func track(
        _ title: String,
        file: BridgeAudioFile?
    ) -> BridgeTrackUserEdit {
        BridgeTrackUserEdit(
            title: title,
            side: 1,
            trackNumber: nil,
            artistNames: [],
            file: file
        )
    }

    /// Twelve paired rows and one file the source's tracklist does not name.
    static let thirteenFileSlots: BridgeSlotTable = {
        var rows: [BridgeTrackSlot] = (1...12)
            .map { index in
                .paired(
                    track: track(
                        "Track \(index)",
                        file: standalone(index).audio
                    ),
                    position: "\(index)",
                    sourceDurationMs: UInt64(200_000 + index * 1000),
                    file: standalone(index)
                )
            }
        rows.append(
            .fileOnly(
                track: track("", file: standalone(13).audio),
                file: standalone(13)
            )
        )
        return BridgeSlotTable(
            rows: rows,
            reconciliation: .moreFiles(files: 13, tracks: 12),
            audio: (1...13).map(standalone)
        )
    }()

    /// The editor the same pick seeds: thirteen rows, the last one unnamed.
    static let thirteenFileEdit = BridgeRawReleaseEdit(
        albumTitle: "Walkthrough",
        albumArtistText: "Artist Name",
        pressing: BridgeRawPressingEdit(
            year: "1996",
            format: "CD",
            label: "",
            catalogNumber: "",
            country: "",
            barcode: ""
        ),
        tracks: (1...13)
            .map { index in
                BridgeRawTrackEdit(
                    id: "import-track-\(index)",
                    title: index <= 12 ? "Track \(index)" : "",
                    artistText: "",
                    side: 1,
                    trackNumber: nil,
                    file: standalone(index).audio
                )
            }
    )

    // MARK: - One container, before and after its sheet is bound

    static let containerId = "Walkthrough.flac"
    static let sheetId = "Walkthrough.cue"
    static let containerPath = "/tmp/walkthrough/Walkthrough.flac"

    /// The sheet describes nothing, so the folder offers the container as one
    /// track and the release gets one slot.
    static let oneSlotTable = BridgeSlotTable(
        rows: [
            .paired(
                track: track("Track 1", file: .standalone(fileId: containerId)),
                position: "1",
                sourceDurationMs: 201_000,
                file: containerFile
            )
        ],
        reconciliation: .moreTracks(files: 1, tracks: 12),
        audio: [containerFile]
    )

    static let containerFile = BridgeSlotFile(
        audio: .standalone(fileId: containerId),
        name: containerId,
        size: 380_000_000,
        localPath: containerPath,
        probedDurationMs: 2_400_000,
        span: .whole
    )

    /// One slice per track the bound sheet carves, spanning the container.
    static func slice(_ index: Int) -> BridgeSlotFile {
        BridgeSlotFile(
            audio: .sheetSlice(
                fileId: containerId,
                sheetId: sheetId,
                index: UInt32(index)
            ),
            name: containerId,
            size: 380_000_000,
            localPath: containerPath,
            probedDurationMs: UInt64(200_000 + index * 1000),
            span: index == 0
                ? .containerStart : (index == 11 ? .containerEnd : .containerMiddle)
        )
    }

    /// The same folder once the sheet is bound: twelve slots out of one file.
    static let twelveSlotTable = BridgeSlotTable(
        rows: (0..<12)
            .map { index in
                .paired(
                    track: track(
                        "Track \(index + 1)",
                        file: slice(index).audio
                    ),
                    position: "\(index + 1)",
                    sourceDurationMs: UInt64(200_000 + index * 1000),
                    file: slice(index)
                )
            },
        reconciliation: .agrees(count: 12),
        audio: (0..<12).map(slice)
    )

    static func edit(
        titles: [String],
        files: [BridgeAudioFile?]
    ) -> BridgeRawReleaseEdit {
        var values = thirteenFileEdit
        values.tracks = titles.indices.map { index in
            BridgeRawTrackEdit(
                id: "import-track-\(index)",
                title: titles[index],
                artistText: "",
                side: 1,
                trackNumber: nil,
                file: files[index]
            )
        }
        return values
    }

    static let emptyFiles = BridgeCandidateFiles(
        files: [],
        formatLabel: "FLAC",
        collapsedDirectories: []
    )

    /// A store holding one folder candidate with `slots` and `edit` on it —
    /// the state a picked release leaves behind.
    @MainActor
    static func store(
        slots: BridgeSlotTable?,
        edit: BridgeRawReleaseEdit?
    ) -> ImportStore {
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
        candidate.mode = .confirming
        candidate.slots = slots
        candidate.editValues = edit
        store.folderCandidates[candidate.key] = candidate
        return store
    }

    /// Whether bae-core can shape this editor state into a savable release —
    /// the only thing standing between the commit bar's button and an import.
    static func isCommittable(_ edit: BridgeRawReleaseEdit) -> Bool {
        if case .valid = shapeReleaseEdit(raw: edit) { return true }
        return false
    }

    /// The model as the pane builds it for the store's one candidate.
    @MainActor
    static func model(of store: ImportStore) -> ImportMappingModel {
        let candidate = store.folderCandidates[candidateKey]
        return ImportMappingModel(
            files: candidate?.files ?? emptyFiles,
            slots: candidate?.slots,
            edit: candidate?.editValues
        )
    }
}
