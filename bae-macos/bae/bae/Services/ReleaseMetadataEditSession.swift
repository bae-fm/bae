import BaeKit
import Observation

/// What one session core opened on a release does. Every editor holds its
/// own, so two editors on one release never share an apply or a pick; core
/// keeps the session for as long as these commands are held, and drops it,
/// writing nothing, once they are not.
struct ReleaseEditSessionCommands: Sendable {
    /// Apply a catalog release to the session; the seed comes back filled
    /// with its every value, held until the save.
    let apply:
        @Sendable (BridgePressingLink) async throws -> BridgeReleaseEditSeed
    /// Pick the album a save files the release under (`nil`: the one its
    /// records name); returns the album the session files it under now.
    let setAlbum: @Sendable (String?) async throws -> BridgeDraftAlbum
    /// Save the session with the form; returns the album the release is on
    /// after it. Refused, writing nothing, when the release changed after
    /// the session opened on it.
    let save: @Sendable (BridgeRawReleaseEdit) async throws -> String
}

/// A session core just opened on a release, and the release as the editor
/// first shows it.
struct OpenedReleaseEdit: Sendable {
    let seed: BridgeReleaseEditSeed
    let session: ReleaseEditSessionCommands
}

/// What a release editor asks core to do beyond its open session, each by
/// release id.
struct ReleaseEditCommands {
    /// Open a session of the editor's own: the release as the library holds
    /// it, with nothing applied or picked.
    let open: @Sendable (String) async throws -> OpenedReleaseEdit
    let searchAlbums: @MainActor (String) async throws -> [BridgeAlbumChoice]
    /// The release's cover as it stands now; the session stays as it is.
    let cover: @Sendable (String) async throws -> BridgeImageRef?
}

/// One release editor's working state, behind the one release editor in the
/// Library sheet and the import pane of an imported folder. It owns the
/// working form and its save; its session in core holds the album the person
/// picked and what a catalog applied in it until that save.
@MainActor
@Observable
final class ReleaseMetadataEditSession {
    private enum Operation {
        case saving
        case cancelling
        case pickingAlbum
        case applying
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
    /// The session in core this editor works in; replaced by a fresh one
    /// whenever the editor opens the release again.
    private var core: ReleaseEditSessionCommands
    private var operationTask: Task<Void, Never>?
    private var operationSerial = 0

    init(
        releaseId: String,
        opened: OpenedReleaseEdit,
        commands: ReleaseEditCommands
    ) {
        self.releaseId = releaseId
        cover = opened.seed.cover
        display = opened.seed.display
        album = opened.seed.album
        form = opened.seed.edit
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

    /// Show the cover the release has now: the cover picker writes its
    /// choice at once, outside the session.
    func refreshCover() async throws {
        cover = try await commands.cover(releaseId)
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
            guard case .valid = shapeReleaseEdit(raw: form) else {
                finish(serial)
                return
            }
            let submittedForm = form
            let submittedRevision = formRevision
            do {
                let albumId = try await core.save(submittedForm)
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

    /// Apply the catalog release `link` picks: every value the form, the
    /// tracks and the album show becomes what it gives, unsaved until Save.
    func apply(_ link: BridgePressingLink) {
        let serial = begin(.applying)
        failure = nil
        operationTask = Task { @MainActor in
            await editingCommands.commitActiveEdits()
            guard !Task.isCancelled else {
                finish(serial)
                return
            }
            do {
                let seed = try await core.apply(link)
                try Task.checkCancellation()
                show(seed)
                hasChanges = true
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
                album = try await core.setAlbum(albumId)
                hasChanges = true
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

    /// Everything the editor shows, from the session just opened, with no
    /// changes made: that session is the one the editor works in from now.
    private func show(_ opened: OpenedReleaseEdit) {
        core = opened.session
        show(opened.seed)
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
