import BaeKit
import OSLog
import Observation

/// What a release edit session asks core to do, each by release id.
struct ReleaseEditCommands {
    /// Open the session afresh: the release as the library holds it, with
    /// nothing applied or picked.
    let open: @Sendable (String) async throws -> BridgeReleaseEditSeed
    /// Pick the album a save files the release under (`nil`: the one its
    /// records name); returns the album the session files it under now.
    let setAlbum: @Sendable (String, String?) async throws -> BridgeDraftAlbum
    let searchAlbums: @MainActor (String) async throws -> [BridgeAlbumChoice]
    /// Save the session with the form; returns the album the release is on
    /// after it.
    let save: @Sendable (String, BridgeRawReleaseEdit) async throws -> String
    /// Drop the session: nothing is written.
    let discard: @Sendable (String) async throws -> Void
}

/// One persisted release-edit session shared by Import Done and the Library
/// modal. It owns the working form and its save; core holds the album the
/// person picked and what a catalog applied in the session until that save.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private static let logger = Logger.bae("ReleaseMetadataEditSession")

    private enum Operation {
        case saving
        case cancelling
        case pickingAlbum
    }

    let releaseId: String
    private(set) var cover: BridgeImageRef?
    private(set) var display: BridgeReleaseEditDisplayContext
    /// The library album a save files the release under.
    private(set) var album: BridgeDraftAlbum
    let editingCommands = EditingCommitCommands()

    private(set) var form: BridgeRawReleaseEdit
    private var operation: Operation?
    private(set) var failure: DisplayError?
    private(set) var hasChanges = false

    private var formRevision = 0
    private let commands: ReleaseEditCommands
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0

    init(
        releaseId: String,
        seed: BridgeReleaseEditSeed,
        commands: ReleaseEditCommands
    ) {
        self.releaseId = releaseId
        cover = seed.cover
        display = seed.display
        album = seed.album
        form = seed.edit
        self.commands = commands
    }

    /// What the album row does: pick a library album, or leave it to the
    /// release's records, and find albums to pick.
    func albumActions(viewInLibrary: @escaping (String) -> Void)
        -> DraftAlbumActions
    {
        DraftAlbumActions(
            pick: { [weak self] albumId in self?.pickAlbum(albumId) },
            search: commands.searchAlbums,
            viewInLibrary: viewInLibrary
        )
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

    /// Save the session, then reopen it from what the library now holds.
    /// `onSuccess` receives the album the release is on after the save.
    func save(onSuccess: @escaping @MainActor @Sendable (String) -> Void) {
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
                let albumId = try await commands.save(releaseId, submittedForm)
                try Task.checkCancellation()
                if formRevision == submittedRevision {
                    show(try await commands.open(releaseId))
                }
                else {
                    hasChanges = true
                }
                finish(serial)
                onSuccess(albumId)
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

    /// Discard every change: core drops the session and the editor reopens
    /// on the release as the library holds it.
    func cancelChanges() {
        let serial = begin(.cancelling)
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                show(try await commands.open(releaseId))
                failure = nil
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Pick the album a save files the release under, or (`nil`) leave it to
    /// its records.
    func pickAlbum(_ albumId: String?) {
        let serial = begin(.pickingAlbum)
        failure = nil
        operationTask = Task { @MainActor in
            do {
                album = try await commands.setAlbum(releaseId, albumId)
                hasChanges = true
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Stop the session's work and drop what core holds for it: the editor
    /// is going away unsaved.
    func close() {
        cancelTasks()
        let commands = commands
        let releaseId = releaseId
        Task {
            do { try await commands.discard(releaseId) }
            catch {
                Self.logger.error(
                    "Could not drop the edit session on \(releaseId): \(error)"
                )
            }
        }
    }

    func cancelTasks() {
        operationSerial += 1
        operationTask?.cancel()
        operationTask = nil
        operation = nil
    }

    /// Everything the editor shows, from `seed`, with no changes made.
    private func show(_ seed: BridgeReleaseEditSeed) {
        cover = seed.cover
        display = seed.display
        album = seed.album
        form = seed.edit
        formRevision += 1
        hasChanges = false
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
