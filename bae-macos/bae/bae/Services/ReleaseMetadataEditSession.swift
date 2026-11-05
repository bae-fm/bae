import BaeKit
import Observation

/// What one session core opened on a release does. Every editor holds its
/// own, so two editors on one release never share an edit, an apply or a
/// pick; core keeps the session for as long as these commands are held, and
/// drops it, writing nothing, once they are not. Each change returns the
/// session's draft.
struct ReleaseEditSessionCommands: Sendable {
    /// The session's draft now, with the release's cover as it stands.
    let draft: @Sendable () async throws -> BridgeReleaseEditDraft
    let setField:
        @Sendable (BridgeCandidateEditField, String) async throws ->
            BridgeReleaseEditDraft
    let setLabels:
        @Sendable ([BridgeRawLabelEdit]) async throws -> BridgeReleaseEditDraft
    let setBarcodes: @Sendable ([String]) async throws -> BridgeReleaseEditDraft
    let setPressingFact:
        @Sendable (BridgePressingFactEdit) async throws ->
            BridgeReleaseEditDraft
    let setAlbumArtists:
        @Sendable ([BridgeCredited]) async throws -> BridgeReleaseEditDraft
    /// Write one track row's title and artists, by its id.
    let setTrack:
        @Sendable (BridgeRawTrackEdit) async throws -> BridgeReleaseEditDraft
    /// Apply a catalog release to the session, each network read it waits
    /// on written to the apply's stream; its every value replaces the form,
    /// held until the save.
    let apply:
        @Sendable (BridgePressingLink, BridgeCatalogApply?) async throws ->
            BridgeReleaseEditDraft
    /// Pick the album a save files the release under (`nil`: the one its
    /// records name).
    let setAlbum: @Sendable (String?) async throws -> BridgeReleaseEditDraft
    /// Save the session; returns the album the release is on after it.
    /// Refused, writing nothing, when the release changed after the session
    /// opened on it.
    let save: @Sendable () async throws -> String
}

/// A session core just opened on a release, and its draft as the editor
/// first shows it.
struct OpenedReleaseEdit: Sendable {
    let draft: BridgeReleaseEditDraft
    let session: ReleaseEditSessionCommands
}

/// What a release editor asks core to do beyond its open session, each by
/// release id.
struct ReleaseEditCommands {
    /// Open a session of the editor's own: the release as the library holds
    /// it, with nothing edited, applied or picked.
    let open: @Sendable (String) async throws -> OpenedReleaseEdit
    let searchAlbums: @MainActor (String) async throws -> [BridgeAlbumChoice]
    /// A new apply of a catalog release and its steps; `nil` where no bridge
    /// is behind the editor.
    let catalogApply: @Sendable () -> CatalogApplySteps?
}

/// One release editor's working state, behind the one release editor in the
/// Library sheet and the import pane of an imported folder: the draft its
/// session in core returned last, and the operations it sends that session.
/// Every change goes to core as the person makes it, one after another in
/// the order made, and the draft core returns is what the editor shows.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case cancelling
        case pickingAlbum
        /// Applying a catalog release, whose steps are followed while it
        /// runs; `nil` where no bridge is behind the editor.
        case applying(CatalogApplySteps?)
    }

    let releaseId: String
    private(set) var draft: BridgeReleaseEditDraft
    let editingCommands = EditingCommitCommands()

    private var operation: Operation?
    private(set) var failure: DisplayError?

    private let commands: ReleaseEditCommands
    /// The session in core this editor works in; replaced by a fresh one
    /// whenever the editor opens the release again.
    private var core: ReleaseEditSessionCommands
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0
    /// The last change sent to core; the next waits for it.
    private var lastChange: Task<Void, Never>?

    init(
        releaseId: String,
        opened: OpenedReleaseEdit,
        commands: ReleaseEditCommands
    ) {
        self.releaseId = releaseId
        draft = opened.draft
        core = opened.session
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
                await self?.write { try await $0.setField(field, value) }
            },
            setLabels: { [weak self] labels in
                await self?.write { try await $0.setLabels(labels) }
            },
            setBarcodes: { [weak self] barcodes in
                await self?.write { try await $0.setBarcodes(barcodes) }
            },
            setPressingFact: { [weak self] fact in
                await self?.write { try await $0.setPressingFact(fact) }
            },
            setAlbumArtists: { [weak self] assignments in
                await self?.write { try await $0.setAlbumArtists(assignments) }
            }
        )
    }

    func updateTrack(_ edited: BridgeRawTrackEdit) async {
        await write { try await $0.setTrack(edited) }
    }

    /// Show the cover the release has now: the cover picker writes its
    /// choice at once, outside the session.
    func refreshCover() async throws {
        try await serially { [self] in draft = try await core.draft() }
    }

    /// Save the session, then open a fresh one on what the library now
    /// holds. `onSuccess` receives the album the release is on after the
    /// save. A release another editor or device changed after this session
    /// opened is refused, writing nothing; Cancel opens it as it is now.
    func save(onSuccess: @escaping @MainActor @Sendable (String) -> Void) {
        let serial = begin(.saving)
        failure = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                let albumId = try await serially { [self] () -> String? in
                    // The edits just committed are in the draft now.
                    guard draft.invalid == nil else { return nil }
                    let albumId = try await core.save()
                    try Task.checkCancellation()
                    show(try await commands.open(releaseId))
                    return albumId
                }
                finish(serial)
                if let albumId { onSuccess(albumId) }
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

    /// Discard every change: the editor opens a fresh session on the release
    /// as the library holds it, and core drops this one.
    func cancelChanges() {
        let serial = begin(.cancelling)
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                try await serially { [self] in
                    show(try await commands.open(releaseId))
                }
                failure = nil
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Apply the catalog release `link` picks: every value the form, the
    /// tracks and the album show becomes what it gives, unsaved until Save.
    func apply(_ link: BridgePressingLink) {
        let steps = commands.catalogApply()
        let apply = steps?.apply
        let serial = begin(.applying(steps))
        failure = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                try await serially { [self] in
                    draft = try await core.apply(link, apply)
                }
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
                try await serially { [self] in
                    draft = try await core.setAlbum(albumId)
                }
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    func cancelTasks() {
        operationSerial += 1
        operationTask?.cancel()
        operationTask = nil
        operation = nil
    }

    /// Send one change of the form to core and show the draft it returns.
    private func write(
        _ change:
            @escaping @MainActor (ReleaseEditSessionCommands) async throws ->
            BridgeReleaseEditDraft
    ) async {
        do {
            try await serially { [self] in draft = try await change(core) }
            failure = nil
        }
        catch is CancellationError {}
        catch {
            failure = DisplayError(error)
        }
    }

    /// Run `step` once the change sent before it has finished, so core takes
    /// the editor's changes in the order they were made. Cancelling the
    /// caller cancels the step.
    private func serially<Value: Sendable>(
        _ step: @escaping @MainActor () async throws -> Value
    ) async throws -> Value {
        let previous = lastChange
        let task = Task { @MainActor in
            await previous?.value
            try Task.checkCancellation()
            return try await step()
        }
        lastChange = Task { _ = await task.result }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    /// The draft of the session just opened: that session is the one the
    /// editor works in from now.
    private func show(_ opened: OpenedReleaseEdit) {
        core = opened.session
        draft = opened.draft
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
            uniqueKeysWithValues: draft.edit.tracks.map { ($0.id, $0) }
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
        return draft.display.sides.map { side in
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

    /// Why the form can't be saved, as core says; `nil` while it can.
    var validationMessage: String? {
        draft.invalid?.localizedMessage
    }

    var isBusy: Bool {
        operation != nil
    }

    /// The network read a catalog apply waits on, for the line that names
    /// it; `nil` while no apply runs.
    var applySteps: LatestSource<BridgeCatalogApplyStep>? {
        guard case .applying(let steps) = operation else { return nil }
        return steps?.steps
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
