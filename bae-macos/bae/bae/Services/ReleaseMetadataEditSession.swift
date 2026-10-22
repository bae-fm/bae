import BaeKit
import Observation

/// One persisted release-edit session shared by Import Done and the Library
/// modal. It owns the working form and its save; core holds what a catalog
/// applied in the session until that save.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case cancelling
    }

    let releaseId: String
    private(set) var cover: BridgeImageRef?
    let display: BridgeReleaseEditDisplayContext
    let editingCommands = EditingCommitCommands()

    private(set) var form: BridgeRawReleaseEdit
    private var operation: Operation?
    private(set) var failure: DisplayError?
    private(set) var hasChanges = false

    private var persistedForm: BridgeRawReleaseEdit
    private var formRevision = 0
    private let saveAction:
        @Sendable (String, BridgeRawReleaseEdit) async throws -> Void
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0

    init(
        releaseId: String,
        seed: BridgeReleaseEditSeed,
        save:
            @escaping @Sendable (
                String, BridgeRawReleaseEdit
            ) async throws -> Void
    ) {
        self.releaseId = releaseId
        cover = seed.cover
        display = seed.display
        form = seed.edit
        persistedForm = seed.edit
        saveAction = save
    }

    var fieldWriter: ReleaseFieldWriter {
        ReleaseFieldWriter(
            setField: { [weak self] field, value in
                self?.setField(field, value)
            },
            setLabels: { [weak self] labels in
                self?.setLabels(labels)
            },
            setBarcodes: { [weak self] barcodes in
                self?.setBarcodes(barcodes)
            },
            setPressingFact: { [weak self] fact in
                self?.setPressingFact(fact)
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
        failure = nil
    }

    func updateCover(_ cover: BridgeImageRef?) {
        self.cover = cover
    }

    func save(onSuccess: @escaping @MainActor @Sendable () -> Void) {
        let serial = begin(.saving)
        failure = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            guard case .valid = shapeReleaseEdit(raw: form) else {
                finish(serial)
                return
            }
            let submittedForm = form
            let submittedRevision = formRevision
            do {
                try await saveAction(releaseId, submittedForm)
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
                failure = DisplayError(error)?
                    .rewording {
                        String(localized: "Save failed: \($0)")
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
            failure = nil
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
        }
        formRevision += 1
        hasChanges = true
        failure = nil
    }

    private func setLabels(_ labels: [BridgeRawLabelEdit]) {
        form.pressing.labels = labels
        formRevision += 1
        hasChanges = true
        failure = nil
    }

    private func setBarcodes(_ barcodes: [String]) {
        form.pressing.barcodes = barcodes
        formRevision += 1
        hasChanges = true
        failure = nil
    }

    private func setPressingFact(_ fact: BridgePressingFactEdit) {
        form.pressing.facts = bridgeApplyPressingFact(
            facts: form.pressing.facts,
            edit: fact
        )
        formRevision += 1
        hasChanges = true
        failure = nil
    }

    private func setAlbumArtists(_ assignments: [BridgeCredited]) {
        form.albumArtistAssignments = assignments
        formRevision += 1
        hasChanges = true
        failure = nil
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
    /// The tracks as core lays the table out — a side per disc or record
    /// side, and on it a run per file a sheet carves several tracks from —
    /// each beside the form row it edits.
    var trackSides: [ReleaseMetadataTrackSide] {
        let formTracks = Dictionary(
            uniqueKeysWithValues: form.tracks.map { ($0.id, $0) }
        )
        func item(_ context: BridgeReleaseEditTrackContext)
            -> ReleaseMetadataTrackItem
        {
            guard let track = formTracks[context.trackId] else {
                preconditionFailure(
                    "Release edit track \(context.trackId) has no form row"
                )
            }
            return ReleaseMetadataTrackItem(track: track, context: context)
        }
        return display.sides.map { side in
            let groups = side.runs.map { run in
                switch run {
                case .track(let track):
                    ReleaseMetadataTrackGroup(
                        id: track.trackId,
                        sharedSource: nil,
                        tracks: [item(track)]
                    )
                case .sheet(let source, let tracks):
                    ReleaseMetadataTrackGroup(
                        // Core never lays out a run without tracks.
                        id: "source:\(source.fileId):\(tracks[0].trackId)",
                        sharedSource: source,
                        tracks: tracks.map(item)
                    )
                }
            }
            // Nor a side without runs.
            return ReleaseMetadataTrackSide(
                id: "side:\(groups[0].id)",
                headerText: side.side.headerText(key: side.headerKey),
                groups: groups
            )
        }
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
}

struct ReleaseMetadataTrackItem: Identifiable {
    let track: BridgeRawTrackEdit
    let context: BridgeReleaseEditTrackContext

    var id: String { track.id }
}

/// A run of tracks the table draws together: one track naming its own
/// source, or the tracks a sheet carves from the one file named above them.
struct ReleaseMetadataTrackGroup: Identifiable {
    let id: String
    let sharedSource: BridgeReleaseEditTrackSource?
    let tracks: [ReleaseMetadataTrackItem]
}

/// One side of the release — a disc of a multi-disc set, a side of a record —
/// and the runs of tracks on it.
struct ReleaseMetadataTrackSide: Identifiable {
    let id: String
    /// "Disc 2" / "Side B", or empty for a release with one flat side, which
    /// has no header to draw.
    let headerText: String
    let groups: [ReleaseMetadataTrackGroup]
}
