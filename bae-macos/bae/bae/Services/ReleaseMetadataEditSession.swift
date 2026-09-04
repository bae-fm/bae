import BaeKit
import Observation

/// One persisted release-edit session shared by Import Done and the Library
/// modal. It owns the working form and every save/reset transition.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case resetting
        case cancelling
    }

    let releaseId: String
    private(set) var cover: BridgeImageRef?
    let display: BridgeReleaseEditDisplayContext
    let canResetToSource: Bool
    let editingCommands = EditingCommitCommands()

    private(set) var form: BridgeRawReleaseEdit
    private var operation: Operation?
    private(set) var failureMessage: String?
    private(set) var hasChanges = false

    private var persistedForm: BridgeRawReleaseEdit
    private var formRevision = 0
    private let trackContextById: [String: BridgeReleaseEditTrackContext]
    private let saveAction:
        @Sendable (String, BridgeReleaseUserEdit) async throws -> Void
    private let resetAction:
        @Sendable (String) async throws -> BridgeRawReleaseEdit
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0

    init(
        releaseId: String,
        seed: BridgeReleaseEditSeed,
        save:
            @escaping @Sendable (
                String, BridgeReleaseUserEdit
            ) async throws -> Void,
        reset: @escaping @Sendable (String) async throws -> BridgeRawReleaseEdit
    ) {
        self.releaseId = releaseId
        cover = seed.cover
        display = seed.display
        canResetToSource = seed.canResetToSource
        form = seed.edit
        persistedForm = seed.edit
        trackContextById = Dictionary(
            uniqueKeysWithValues: seed.display.tracks.map {
                ($0.trackId, $0)
            }
        )
        saveAction = save
        resetAction = reset
    }

    var fieldWriter: ReleaseFieldWriter {
        ReleaseFieldWriter(
            setField: { [weak self] field, value in
                self?.setField(field, value)
            },
            setAlbumArtists: { [weak self] assignments in
                self?.setAlbumArtists(assignments)
            }
        )
    }

    func updateTrack(_ edited: BridgeRawTrackEdit) {
        guard let index = form.tracks.firstIndex(where: { $0.id == edited.id })
        else {
            preconditionFailure(
                "Release edit changed unknown track \(edited.id)"
            )
        }
        form.tracks[index] = edited
        formRevision += 1
        hasChanges = true
        failureMessage = nil
    }

    func updateCover(_ cover: BridgeImageRef?) {
        self.cover = cover
    }

    func save(onSuccess: @escaping @MainActor @Sendable () -> Void) {
        let serial = begin(.saving)
        failureMessage = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            guard case .valid(let edit) = shapeReleaseEdit(raw: form) else {
                finish(serial)
                return
            }
            let submittedForm = form
            let submittedRevision = formRevision
            do {
                try await saveAction(releaseId, edit)
                try Task.checkCancellation()
                persistedForm = submittedForm
                hasChanges = formRevision != submittedRevision
                finish(serial)
                onSuccess()
            }
            catch is CancellationError {
                finish(serial)
            }
            catch {
                guard serial == operationSerial else { return }
                finish(serial)
                failureMessage = error.displayLine.map {
                    String(localized: "Save failed: \($0)")
                }
            }
        }
    }

    func resetToSource() {
        let serial = begin(.resetting)
        failureMessage = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                let resetForm = try await resetAction(releaseId)
                try Task.checkCancellation()
                form = resetForm
                formRevision += 1
                hasChanges = true
                finish(serial)
            }
            catch is CancellationError {
                finish(serial)
            }
            catch {
                guard serial == operationSerial else { return }
                finish(serial)
                failureMessage = error.displayLine.map {
                    String(localized: "Reset failed: \($0)")
                }
            }
        }
    }

    func cancelChanges() {
        let serial = begin(.cancelling)
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            form = persistedForm
            formRevision += 1
            hasChanges = false
            failureMessage = nil
            finish(serial)
        }
    }

    func cancelTasks() {
        operationSerial += 1
        operationTask?.cancel()
        operationTask = nil
        operation = nil
    }

    private func setField(_ field: BridgeCandidateEditField, _ value: String) {
        switch field {
        case .albumTitle: form.albumTitle = value
        case .albumYear: form.albumYear = value
        case .pressingYear: form.pressing.year = value
        case .format: form.pressing.format = value
        case .label: form.pressing.label = value
        case .catalogNumber: form.pressing.catalogNumber = value
        case .country: form.pressing.country = value
        case .barcode: form.pressing.barcode = value
        }
        formRevision += 1
        hasChanges = true
        failureMessage = nil
    }

    private func setAlbumArtists(_ assignments: [BridgeArtistAssignment]) {
        form.albumArtistAssignments = assignments
        formRevision += 1
        hasChanges = true
        failureMessage = nil
    }

    private func begin(_ next: Operation) -> Int {
        operationSerial += 1
        operationTask?.cancel()
        operation = next
        return operationSerial
    }

    private func finish(_ serial: Int) {
        guard serial == operationSerial else { return }
        operation = nil
        operationTask = nil
    }
}

extension ReleaseMetadataEditSession {
    var trackItems: [ReleaseMetadataTrackItem] {
        form.tracks.enumerated()
            .map { index, track in
                guard let context = trackContextById[track.id] else {
                    preconditionFailure(
                        "Release edit track \(track.id) has no display context"
                    )
                }
                return ReleaseMetadataTrackItem(
                    index: index,
                    track: track,
                    context: context
                )
            }
    }

    /// The tracks as the table lays them out: a run per side, and within it a
    /// run per sheet the tracks are carved from. The same shape the import
    /// mapping table draws, so a release reads the same before and after it
    /// is imported.
    var trackSides: [ReleaseMetadataTrackSide] {
        var sides: [ReleaseMetadataTrackSide] = []
        for item in trackItems {
            if let last = sides.last, last.side == item.context.side {
                sides[sides.count - 1].append(item)
            }
            else {
                sides.append(
                    ReleaseMetadataTrackSide(
                        id: "side:\(item.id)",
                        side: item.context.side,
                        headerText: item.context.side.headerText(
                            key: item.context.sideHeaderKey
                        ),
                        groups: []
                    )
                )
                sides[sides.count - 1].append(item)
            }
        }
        return sides
    }

    var validationMessage: String? {
        if case .invalid(let reason) = shapeReleaseEdit(raw: form) {
            return reason.localizedMessage
        }
        return nil
    }

    var isBusy: Bool {
        operation != nil
    }

    var isSaving: Bool {
        operation == .saving
    }

    var isResetting: Bool {
        operation == .resetting
    }
}

struct ReleaseMetadataTrackItem: Identifiable {
    let index: Int
    let track: BridgeRawTrackEdit
    let context: BridgeReleaseEditTrackContext

    var id: String { track.id }

    var sharedCueSource: BridgeReleaseEditTrackSource? {
        guard context.sources.count == 1,
            let source = context.sources.first,
            case .cue = source.layout
        else { return nil }
        return source
    }
}

struct ReleaseMetadataTrackGroup: Identifiable {
    let id: String
    let sharedSource: BridgeReleaseEditTrackSource?
    var tracks: [ReleaseMetadataTrackItem]
}

/// One side of the release — a disc of a multi-disc set, a side of a record —
/// and the runs of tracks on it.
struct ReleaseMetadataTrackSide: Identifiable {
    let id: String
    let side: BridgeTrackSide
    /// "Disc 2" / "Side B", or empty for a release with one flat side, which
    /// has no header to draw.
    let headerText: String
    var groups: [ReleaseMetadataTrackGroup]

    /// Add the next track in table order: onto the run of the sheet it is
    /// carved from when that run is the last one, else as a run of its own.
    mutating func append(_ item: ReleaseMetadataTrackItem) {
        guard let cueSource = item.sharedCueSource else {
            groups.append(
                ReleaseMetadataTrackGroup(
                    id: item.id,
                    sharedSource: nil,
                    tracks: [item]
                )
            )
            return
        }
        if let last = groups.last,
            last.sharedSource?.fileId == cueSource.fileId
        {
            groups[groups.count - 1].tracks.append(item)
        }
        else {
            groups.append(
                ReleaseMetadataTrackGroup(
                    id: "source:\(cueSource.fileId):\(item.id)",
                    sharedSource: cueSource,
                    tracks: [item]
                )
            )
        }
    }
}
