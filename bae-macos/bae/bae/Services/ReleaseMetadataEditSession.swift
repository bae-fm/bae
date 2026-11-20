import BaeKit
import Observation

/// What the form a release editor holds does. Every editor holds its own,
/// so two editors on one release never share an edit, an apply or a pick,
/// and an editor that closes drops its form, writing nothing to the release.
/// Each change replaces the form and points the editor's draft read at it;
/// an apply, a save and a Cancel return once core has answered. Each command
/// that can change the form's values returns them, which the editor draws.
struct ReleaseEditFormCommands: Sendable {
    let setField:
        @Sendable (BridgeCandidateEditField, String) async throws ->
            BridgeRawReleaseEdit
    let setLabels:
        @Sendable ([BridgeRawLabelEdit]) async throws -> BridgeRawReleaseEdit
    let setBarcodes: @Sendable ([String]) async throws -> BridgeRawReleaseEdit
    let setPressingFact:
        @Sendable (BridgePressingFactEdit) async throws -> BridgeRawReleaseEdit
    let setAlbumArtists:
        @Sendable ([BridgeCredited]) async throws -> BridgeRawReleaseEdit
    /// Change one track row's title and artists, by its id.
    let setTrack:
        @Sendable (BridgeRawTrackEdit) async throws -> BridgeRawReleaseEdit
    /// Apply a catalog release to the form, each network read it waits on
    /// written to the apply's stream; its every value replaces the form's,
    /// held until the save.
    let apply:
        @Sendable (BridgePressingLink, BridgeCatalogApply?) async throws ->
            BridgeRawReleaseEdit
    /// Record what the person picked for the album a save files the release
    /// under.
    let setAlbum: @Sendable (BridgeAlbumPick) async throws -> Void
    /// Record the disc the person picked for a CUE sheet, by the sheet's
    /// file id: a save moves the sheet's tracks there.
    let setSheetDisc: @Sendable (String, UInt32) async throws -> Void
    /// Discard every change: the editor opens its form again on the release
    /// as the library holds it now.
    let cancel: @Sendable () async throws -> BridgeRawReleaseEdit
    /// Save the form, and hold the form opened on what the save wrote. Core
    /// refuses the save, and writes nothing, when the release changed after
    /// the form opened on it.
    let save: @Sendable () async throws -> SavedReleaseEdit
}

/// What a release editor's save returns: the album the release is on after
/// the save, and the values of the form opened on what the save wrote.
struct SavedReleaseEdit: Sendable {
    let albumId: String
    let values: BridgeRawReleaseEdit
}

/// A form an editor just opened on a release: its values, the subscription
/// that sends the form's draft, and the commands the form takes.
struct OpenedReleaseEdit: Sendable {
    /// The values of the form as it opened.
    let values: BridgeRawReleaseEdit
    /// The form's draft at once, then after each change of the form and
    /// each write that changes what the draft shows; `nil` once someone
    /// deletes the form's release.
    let drafts: PulledValues<BridgeReleaseEditDraft?>
    let form: ReleaseEditFormCommands
}

/// What a release editor asks core to do beyond its form, each by release
/// id.
struct ReleaseEditCommands {
    /// Open a form of the editor's own: the release as the library holds
    /// it, with nothing edited, applied or picked.
    let open: @Sendable (String) async throws -> OpenedReleaseEdit
    let searchAlbums: @MainActor (String) async throws -> [BridgeAlbumChoice]
    /// A new apply of a catalog release; `nil` where no bridge is behind the
    /// editor.
    let catalogApply: @Sendable () -> (any CatalogApplySource)?
}

/// One release editor's working state, behind the one release editor in the
/// Library sheet and the import pane of an imported folder: the form it
/// holds, through its commands, the values of that form, which the editor
/// draws, and the draft the form's subscription sent last. Every change makes a new form as the person makes it, one after
/// another in the order the person made them. The subscription sends a new
/// draft whenever the form changes or a write changes what the draft shows
/// of the library, such as when someone merges an artist, renames the album
/// or picks a cover.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case cancelling
        case pickingAlbum
        case pickingSheetDisc
        /// Applying a catalog release. The editor's step line shows the
        /// network read the apply waits on; `nil` when no apply runs.
        case applying((any CatalogApplySource)?)
    }

    let releaseId: String
    /// The values of the form the editor holds, as its last command left
    /// them.
    private(set) var values: BridgeRawReleaseEdit
    /// The draft the subscription sent last; `nil` until it sends its first,
    /// and the editor shows nothing until then.
    private(set) var draft: BridgeReleaseEditDraft?
    /// The form's release is gone from the library, so the editor has
    /// nothing to edit and closes.
    private(set) var releaseGone = false
    let editingCommands = EditingCommitCommands()

    private var operation: Operation?
    private(set) var failure: DisplayError?

    private let commands: ReleaseEditCommands
    /// The form this editor holds, for as long as the editor is open.
    private let form: ReleaseEditFormCommands
    @ObservationIgnored
    private var drafts:
        ScreenSubscription<PulledValues<BridgeReleaseEditDraft?>>?
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0
    /// The last change made to the form; the next waits for it.
    private var lastChange: Task<Void, Never>?

    init(
        releaseId: String,
        opened: OpenedReleaseEdit,
        commands: ReleaseEditCommands
    ) {
        self.releaseId = releaseId
        values = opened.values
        form = opened.form
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

    /// Save the form; the editor then holds the form opened on what the
    /// save wrote. `onSuccess` receives the album the release is on after
    /// the save. Core refuses the save, and writes nothing, when another
    /// editor or device changed the release after this form opened; Cancel
    /// opens it as it is now.
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
                let saved = try await serially { [form] in
                    try await form.save()
                }
                values = saved.values
                finish(serial)
                onSuccess(saved.albumId)
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

    /// Discard every change: the editor opens its form again on the release
    /// as the library holds it.
    func cancelChanges() {
        let serial = begin(.cancelling)
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                values = try await serially { [form] in
                    try await form.cancel()
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
                values = try await serially { [form] in
                    try await form.apply(link, apply?.bridgeApply)
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
                try await serially { [form] in try await form.setAlbum(pick) }
            }
            catch is CancellationError {}
            catch {
                failure = DisplayError(error)
            }
            finish(serial)
        }
    }

    /// Record the disc the person picked for the CUE sheet `sheetFileId`.
    func pickSheetDisc(_ sheetFileId: String, disc: UInt32) {
        let serial = begin(.pickingSheetDisc)
        failure = nil
        operationTask = Task { @MainActor in
            do {
                try await serially { [form] in
                    try await form.setSheetDisc(sheetFileId, disc)
                }
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

    /// Make one change of the form, and hold the values it leaves.
    private func write(
        _ change:
            @escaping @MainActor (ReleaseEditFormCommands) async throws ->
            BridgeRawReleaseEdit
    ) async {
        do {
            values = try await serially { [form] in try await change(form) }
            failure = nil
        }
        catch is CancellationError {}
        catch {
            failure = DisplayError(error)
        }
    }

    /// Run `step` once the change made before it has finished, so the form
    /// takes the editor's changes in the order they were made. Cancelling
    /// the caller cancels the step.
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
