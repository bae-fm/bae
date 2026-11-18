import BaeKit
import Observation

/// What one session core opened on a release does. Every editor holds its
/// own, so two editors on one release never share an edit, an apply or a
/// pick; core keeps the session for as long as these commands are held, and
/// deletes it, writing nothing to the release, once they are not. Each
/// command returns once core has written it; the editor reads the session's
/// draft from the draft subscription it opened with.
struct ReleaseEditSessionCommands: Sendable {
    let setField:
        @Sendable (BridgeCandidateEditField, String) async throws -> Void
    let setLabels: @Sendable ([BridgeRawLabelEdit]) async throws -> Void
    let setBarcodes: @Sendable ([String]) async throws -> Void
    let setPressingFact: @Sendable (BridgePressingFactEdit) async throws -> Void
    let setAlbumArtists: @Sendable ([BridgeCredited]) async throws -> Void
    /// Write one track row's title and artists, by its id.
    let setTrack: @Sendable (BridgeRawTrackEdit) async throws -> Void
    /// Apply a catalog release to the session, each network read it waits
    /// on written to the apply's stream; its every value replaces the form,
    /// held until the save.
    let apply:
        @Sendable (BridgePressingLink, BridgeCatalogApply?) async throws -> Void
    /// Record what the person picked for the album a save files the release
    /// under.
    let setAlbum: @Sendable (BridgeAlbumPick) async throws -> Void
    /// Discard everything the session holds: core opens it again on the
    /// release as the library holds it now.
    let cancel: @Sendable () async throws -> Void
    /// Save the session, which core opens again on what it saved; returns
    /// the album the release is on after it. Core refuses the save, and
    /// writes nothing, when the release changed after the session opened on
    /// it.
    let save: @Sendable () async throws -> String
}

/// A session core just opened on a release: the subscription that sends the
/// session's draft, and the commands the session takes.
struct OpenedReleaseEdit: Sendable {
    /// The session's draft at once, then after each write that changes it;
    /// `nil` once someone deletes the session's release.
    let drafts: PulledValues<BridgeReleaseEditDraft?>
    let session: ReleaseEditSessionCommands
}

/// What a release editor asks core to do beyond its open session, each by
/// release id.
struct ReleaseEditCommands {
    /// Open a session of the editor's own: the release as the library holds
    /// it, with nothing edited, applied or picked.
    let open: @Sendable (String) async throws -> OpenedReleaseEdit
    let searchAlbums: @MainActor (String) async throws -> [BridgeAlbumChoice]
    /// A new apply of a catalog release; `nil` where no bridge is behind the
    /// editor.
    let catalogApply: @Sendable () -> (any CatalogApplySource)?
}

/// One release editor's working state, behind the one release editor in the
/// Library sheet and the import pane of an imported folder: the draft its
/// session's subscription sent last, and the operations it sends that
/// session. Every change goes to core as the person makes it, one after
/// another in the order the person made them. The subscription sends a new
/// draft whenever a write changes the session or what the draft shows of the
/// library, such as when someone merges an artist, renames the album or
/// picks a cover.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case cancelling
        case pickingAlbum
        /// Applying a catalog release. The editor's step line shows the
        /// network read the apply waits on; `nil` when no apply runs.
        case applying((any CatalogApplySource)?)
    }

    let releaseId: String
    /// The draft the subscription sent last; `nil` until it sends its first,
    /// and the editor shows nothing until then.
    private(set) var draft: BridgeReleaseEditDraft?
    /// The session's release is gone from the library, so the editor has
    /// nothing to edit and closes.
    private(set) var releaseGone = false
    let editingCommands = EditingCommitCommands()

    private var operation: Operation?
    private(set) var failure: DisplayError?

    private let commands: ReleaseEditCommands
    /// The session in core this editor works in, for as long as the editor
    /// is open.
    private let core: ReleaseEditSessionCommands
    @ObservationIgnored
    private var drafts:
        ScreenSubscription<PulledValues<BridgeReleaseEditDraft?>>?
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
        core = opened.session
        self.commands = commands
        drafts = ScreenSubscription(
            opened.drafts,
            onValue: { [weak self] value in
                guard let self else { return }
                if let value {
                    draft = value
                }
                else {
                    releaseGone = true
                }
            },
            onError: { [weak self] error in self?.failure = DisplayError(error)
            }
        )
    }

    /// What the album row does: pick a library album or a new album, or
    /// leave it to the release's records, and find albums to pick.
    var albumActions: DraftAlbumActions {
        DraftAlbumActions(
            pick: { [weak self] pick in self?.pickAlbum(pick) },
            search: commands.searchAlbums
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

    /// Save the session; core opens it again on what the library now holds
    /// in the same write. `onSuccess` receives the album the release is on
    /// after the save. Core refuses the save, and writes nothing, when another
    /// editor or device changed the release after this session opened;
    /// Cancel opens it as it is now.
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
                let albumId = try await serially { [core] in
                    try await core.save()
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

    /// Discard every change: core opens the session again on the release as
    /// the library holds it.
    func cancelChanges() {
        let serial = begin(.cancelling)
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                try await serially { [core] in try await core.cancel() }
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
        let apply = commands.catalogApply()
        let serial = begin(.applying(apply))
        failure = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                try await serially { [core] in
                    try await core.apply(link, apply?.bridgeApply)
                }
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Record what the person picked for the album a save files the release
    /// under.
    func pickAlbum(_ pick: BridgeAlbumPick) {
        let serial = begin(.pickingAlbum)
        failure = nil
        operationTask = Task { @MainActor in
            do {
                try await serially { [core] in try await core.setAlbum(pick) }
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Stop the editor's work and its read of the draft: the editor no
    /// longer shows.
    func cancelTasks() {
        operationSerial += 1
        operationTask?.cancel()
        operationTask = nil
        operation = nil
        drafts?.stop()
        drafts = nil
    }

    /// Send one change of the form to core.
    private func write(
        _ change:
            @escaping @MainActor (ReleaseEditSessionCommands) async throws ->
            Void
    ) async {
        do {
            try await serially { [core] in try await change(core) }
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
    /// Why the form can't be saved, as core says; `nil` while it can.
    var validationMessage: String? {
        draft?.invalid?.localizedMessage
    }

    var isBusy: Bool {
        operation != nil
    }

    /// The catalog apply that runs, for the line that names the network
    /// read it waits on; `nil` while none runs.
    var catalogApplying: CatalogApplying? {
        guard case .applying(let apply) = operation else { return nil }
        return CatalogApplying(apply: apply)
    }
}

/// A catalog apply the release editor runs: the bridge's apply, whose row
/// holds the network read it waits on, or `nil` where no bridge is behind
/// the editor.
struct CatalogApplying {
    let apply: (any CatalogApplySource)?
}
