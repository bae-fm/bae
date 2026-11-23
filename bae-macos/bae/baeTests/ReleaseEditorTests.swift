import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Release editor")
struct ReleaseEditorTests {
    @Test("Modal size follows the host with minimum and maximum bounds")
    func modalSize() {
        #expect(
            ReleaseEditorSheet.modalSize(
                in: CGSize(width: 1_600, height: 1_000)
            ) == CGSize(width: 1_100, height: 920)
        )
        #expect(
            ReleaseEditorSheet.modalSize(
                in: CGSize(width: 820, height: 650)
            ) == CGSize(width: 760, height: 600)
        )
        #expect(
            ReleaseEditorSheet.modalSize(
                in: CGSize(width: 700, height: 500)
            ) == CGSize(width: 700, height: 500)
        )
    }

    @MainActor
    @Test(
        "A save holds the form opened on what it saved, which Cancel returns to"
    )
    func saveAdvancesTheFormCancelReturnsTo() async throws {
        let release = PreviewData.releaseEdit(trackCount: 2)
        let library = PreviewReleaseLibrary(release: release)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release,
            library: library
        )
        await session.fieldWriter.setField(.albumTitle, "Saved title")

        await withCheckedContinuation { continuation in
            session.save { _ in continuation.resume() }
        }

        #expect(await library.open().values.albumTitle == "Saved title")
        #expect(session.values.albumTitle == "Saved title")
        try await Wait.until { session.draft?.changed == false }

        await session.fieldWriter.setField(.albumTitle, "Unsaved title")
        #expect(session.values.albumTitle == "Unsaved title")
        try await Wait.until { session.draft?.changed == true }
        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        #expect(session.values.albumTitle == "Saved title")
        try await Wait.until { session.draft?.changed == false }
    }

    @MainActor
    @Test("Save commits the field that still has focus")
    func saveCommitsFocusedField() async throws {
        var release = PreviewData.releaseEdit(trackCount: 2)
        release.values.albumTitle = "Original title"
        let library = PreviewReleaseLibrary(release: release)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release,
            library: library
        )
        let size = NSSize(width: 900, height: 500)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: session.values,
                pressingOffers: release.draft.pressingOffers,
                albumArtists: release.draft.albumArtists,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                artistSearch: { _ in .fixed([], answers: UnansweredCount()) },
                cover: { EmptyView() },
                audioFacts: { EmptyView() },
                albumRow: { EmptyView() }
            )
            .environment(UiStore())
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)

            let titleField = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == "Original title" }
            )
            #expect(HostedInput.focus(titleField, in: window))
            titleField.stringValue = "Focused title"
            titleField.delegate?.controlTextDidChange?(
                Notification(
                    name: NSControl.textDidChangeNotification,
                    object: titleField
                )
            )
            // The field's focus state follows the responder change a turn later;
            // the save must see the field as focused, as a person's would.
            try await SnapshotTestSupport.settle(host)

            await withCheckedContinuation { continuation in
                session.save { _ in continuation.resume() }
            }

            #expect(await library.open().values.albumTitle == "Focused title")
        }
    }

    @MainActor
    @Test("Tracks sharing one CUE source keep one caption above their rows")
    func sharedCueSourceKeepsCaption() async throws {
        let files = try await trackTitleFrames(width: 1_000, shared: false)
        let shared = try await trackTitleFrames(width: 1_000, shared: true)
        #expect(shared[0].minY > files[0].minY)
        #expect(
            shared[1].minY - shared[0].minY == files[1].minY - files[0].minY
        )
        #expect(shared[0].minX == files[0].minX)
    }

    @MainActor
    private func trackTitleFrames(
        width: CGFloat,
        shared: Bool
    ) async throws -> [CGRect] {
        let release = twoTrackRelease(shared: shared)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release
        )
        // Tall enough that neither layout scrolls: on a Mac that always shows
        // scroll bars, as CI's runner does, the bar narrows a layout that
        // scrolls, and its columns move left of the ones that fit.
        let size = NSSize(width: width, height: 1_200)
        return try await SnapshotTestSupport.withHostedWindow(
            ScrollView {
                MacImportTable(
                    sides: release.draft.sides ?? [],
                    previewingTarget: nil,
                    actions: ReleaseTrackTableActions(
                        sheetCaption: SheetCaptionActions(
                            open: { _, _ in },
                            place: { _, _ in },
                            bind: { _, _, _ in }
                        ),
                        audition: TrackAuditionActions(
                            preview: { _ in },
                            stopPreview: {},
                            playTrack: { _ in }
                        ),
                        editTrack: { await session.updateTrack($0) }
                    ),
                    editingCommands: session.editingCommands
                )
            }
            .environment(Library.stub())
            .environment(UiStore())
            .environment(ImageStore.stub())
            .background(Theme.background)
            .preferredColorScheme(.dark)
            .frame(width: width, height: size.height, alignment: .topLeading),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let fields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
            return try release.values.tracks.map { track in
                let field = try #require(
                    fields.first { $0.stringValue == track.title }
                )
                return field.convert(field.bounds, to: host)
            }
        }
    }

    /// A release of two tracks, each on a file of its own, or both carved by
    /// a sheet from one file when `shared` is true.
    private func twoTrackRelease(shared: Bool) -> PreviewReleaseEdit {
        var release = PreviewData.releaseEdit(trackCount: 2)
        let file = { (index: Int) -> (id: String, name: String) in
            shared
                ? ("disc.flac", "Disc audio.flac")
                : (
                    "track-\(index).flac",
                    "Disc 1/0\(index + 1) A source track with a long filename.flac"
                )
        }
        let rows = release.values.tracks.enumerated()
            .map { index, track in
                PreviewData.releaseEditRow(
                    track,
                    index: index,
                    source: shared
                        ? .sheetSlice(start: BridgeDurationClock.length(ms: 0))
                        : .file(name: file(index).name)
                )
            }
        // Core lays the tracks a sheet carves out as one run under its
        // caption, and every other track as a run of its own.
        release.draft.sides = [
            BridgeTrackTableSide(
                side: .flat,
                runs: shared
                    ? [
                        .sheet(
                            sheet: discSheet(playing: file(0)),
                            rows: rows
                        )
                    ]
                    : rows.map { .track(row: $0) }
            )
        ]
        return release
    }
}

extension ReleaseEditorTests {
    /// The caption of the CUE sheet that carves both tracks from `file`, its
    /// one line offering that file and the release's rip.
    private func discSheet(
        playing file: (id: String, name: String)
    ) -> BridgeSheetGroup {
        let line = BridgeSheetReferenceOptions(
            fileReference: "Disc audio.wav",
            bound: BridgeMappingContainer(fileId: file.id, name: file.name),
            options: [
                BridgeSheetBindingOption(
                    fileId: file.id,
                    name: file.name,
                    offer: .offered
                ),
                BridgeSheetBindingOption(
                    fileId: "disc-rip.flac",
                    name: "Disc audio (rip).flac",
                    offer: .offered
                ),
            ]
        )
        return BridgeSheetGroup(
            sheetId: "disc.cue",
            name: "Disc audio.cue",
            bound: .describes(
                container: BridgeMappingContainer(
                    fileId: file.id,
                    name: file.name
                )
            ),
            boundAudio: .one(reference: line),
            unboundReferences: nil,
            assigned: .disc(1),
            sideChoices: BridgeSheetSideChoice.discs(2)
        )
    }
}

extension ReleaseEditorTests {
    @MainActor
    @Test("cancel restores the saved form")
    func cancelRestoresSavedForm() async throws {
        let release = PreviewData.releaseEdit(trackCount: 2)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release
        )
        await session.fieldWriter.setField(.albumTitle, "Edited title")
        await session.fieldWriter.setPressingFact(
            .area(area: .country(code: "JP"))
        )
        try await Wait.until { session.draft?.changed == true }

        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        try await Wait.until { session.draft?.changed == false }
        #expect(session.values.albumTitle == release.values.albumTitle)
        #expect(session.values.pressing.facts == release.values.pressing.facts)
    }
}

extension ReleaseEditorTests {
    @MainActor
    @Test("a side picked for a sheet reaches the form and shows in the draft")
    func sidePickedForASheetShowsInTheDraft() async throws {
        let release = twoTrackRelease(shared: true)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release
        )
        try await Wait.until { session.draft != nil }

        session.pickSheetPlace("disc.cue", place: .on(side: .firstOf(2)))
        try await Wait.until { !session.isBusy }
        try await Wait.until {
            guard
                case .sheet(let sheet, _) = session.draft?.sides?.first?
                    .runs.first
            else { return false }
            return sheet.assigned == .disc(2)
        }
    }
}

extension ReleaseEditorTests {
    /// The sheet places the form received, in order.
    actor PlaceLog {
        private(set) var places: [String] = []

        func record(_ sheet: String, _ place: BridgeSheetPlace) {
            switch place {
            case .ignored: places.append("\(sheet) ignored")
            case .on(let side):
                places.append("\(sheet) \(side.medium).\(side.side)")
            }
        }
    }

    @MainActor
    @Test("the Ignored choice and a side pick both reach the form in order")
    func ignoredAndSidePicksReachTheForm() async throws {
        let release = twoTrackRelease(shared: true)
        let library = PreviewReleaseLibrary(release: release)
        let core = PreviewReleaseEditSession(release: release)
        let preview = library.commands(of: core)
        let log = PlaceLog()
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            opened: OpenedReleaseEdit(
                values: release.values,
                drafts: core.drafts(),
                form: ReleaseEditFormCommands(
                    setField: preview.setField,
                    setLabels: preview.setLabels,
                    setBarcodes: preview.setBarcodes,
                    setPressingFact: preview.setPressingFact,
                    setAlbumArtists: preview.setAlbumArtists,
                    setTrack: preview.setTrack,
                    apply: preview.apply,
                    applyFileTags: preview.applyFileTags,
                    clear: preview.clear,
                    unlink: preview.unlink,
                    setAlbum: preview.setAlbum,
                    setSheetPlace: { sheet, place in
                        await log.record(sheet, place)
                    },
                    setSheetFile: preview.setSheetFile,
                    cancel: preview.cancel,
                    save: preview.save
                )
            ),
            commands: library.commands()
        )
        try await Wait.until { session.draft != nil }

        session.pickSheetPlace("disc.cue", place: .ignored)
        try await Wait.until { !session.isBusy }
        #expect(await log.places == ["disc.cue ignored"])
        session.pickSheetPlace("disc.cue", place: .on(side: .firstOf(1)))
        try await Wait.until { !session.isBusy }
        #expect(await log.places == ["disc.cue ignored", "disc.cue 1.1"])
    }
}

extension ReleaseEditorTests {
    @MainActor
    @Test("a file picked for a sheet reaches the form and shows in the draft")
    func filePickedForASheetShowsInTheDraft() async throws {
        let release = twoTrackRelease(shared: true)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            release: release
        )
        try await Wait.until { session.draft != nil }

        session.pickSheetFile(
            "disc.cue",
            fileReference: "Disc audio.wav",
            audioFileId: "disc-rip.flac"
        )
        try await Wait.until { !session.isBusy }
        try await Wait.until {
            guard
                case .sheet(let sheet, _) = session.draft?.sides?.first?
                    .runs.first,
                case .describes(let source) = sheet.bound
            else { return false }
            return source.name == "Disc audio (rip).flac"
        }
        #expect(session.draft?.filesToRead == ["Disc audio (rip).flac"])
    }
}

extension ReleaseEditorTests {
    /// The titles the editor's form takes, in the order they reached it.
    /// This actor writes each title to the form's values and judges it in the
    /// draft the editor's subscription reads.
    private actor CoreSession {
        private(set) var titles: [String] = []
        let session: PreviewReleaseEditSession
        private(set) var firstChangeArrived = false
        private var heldFirstChange: CheckedContinuation<Void, Never>?

        init(release: PreviewReleaseEdit) {
            session = PreviewReleaseEditSession(release: release)
        }

        /// Take a title as the form does: blank is not savable. Returns the
        /// form's values.
        func setTitle(_ title: String) async -> BridgeRawReleaseEdit {
            // The first change is held until the test lets it go, so a later
            // one would land first if the editor didn't wait for it.
            if !firstChangeArrived {
                firstChangeArrived = true
                Wait.happened()
                await withCheckedContinuation { heldFirstChange = $0 }
            }
            titles.append(title)
            return await session.change {
                $0.values.albumTitle = title
                $0.draft.invalid = title.isEmpty ? .emptyAlbumTitle : nil
            }
        }

        /// Let the first change take effect.
        func releaseFirstChange() {
            heldFirstChange?.resume()
            heldFirstChange = nil
        }
    }

    @MainActor
    @Test(
        "each change reaches the form in the order the person made it, and the editor shows the form and the draft core sends"
    )
    func changesReachCoreInOrder() async throws {
        let release = PreviewData.releaseEdit(trackCount: 1)
        let core = CoreSession(release: release)
        let library = PreviewReleaseLibrary(release: release)
        let preview = library.commands(of: core.session)
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            opened: OpenedReleaseEdit(
                values: release.values,
                drafts: core.session.drafts(),
                form: ReleaseEditFormCommands(
                    setField: { _, value in await core.setTitle(value) },
                    setLabels: preview.setLabels,
                    setBarcodes: preview.setBarcodes,
                    setPressingFact: preview.setPressingFact,
                    setAlbumArtists: preview.setAlbumArtists,
                    setTrack: preview.setTrack,
                    apply: preview.apply,
                    applyFileTags: preview.applyFileTags,
                    clear: preview.clear,
                    unlink: preview.unlink,
                    setAlbum: preview.setAlbum,
                    setSheetPlace: preview.setSheetPlace,
                    setSheetFile: preview.setSheetFile,
                    cancel: preview.cancel,
                    save: preview.save
                )
            ),
            commands: library.commands()
        )

        let writer = session.fieldWriter
        let first = Task { @MainActor in await writer.setField(.albumTitle, "")
        }
        try await Wait.until { await core.firstChangeArrived }
        let second = Task { @MainActor in
            await writer.setField(.albumTitle, "Album Title")
        }
        await core.releaseFirstChange()
        await first.value
        await second.value
        #expect(await core.titles == ["", "Album Title"])
        #expect(session.values.albumTitle == "Album Title")
        #expect(session.validationMessage == nil)

        await session.fieldWriter.setField(.albumTitle, "")
        try await Wait.until { session.validationMessage != nil }
        #expect(session.draft?.changed == true)
    }
}
