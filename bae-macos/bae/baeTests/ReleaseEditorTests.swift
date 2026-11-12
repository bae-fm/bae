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
    @Test("Done and Library save through one persisted release session")
    func persistedReleaseSessionSavesAndAdvancesItsCancelPoint() async throws {
        let draft = PreviewData.releaseEditDraft(trackCount: 2)
        let library = PreviewReleaseLibrary(draft: draft)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            draft: draft,
            library: library
        )
        await session.fieldWriter.setField(.albumTitle, "Saved title")

        await withCheckedContinuation { continuation in
            session.save { _ in continuation.resume() }
        }

        #expect(await library.open().edit.albumTitle == "Saved title")
        #expect(!session.draft.changed)

        await session.fieldWriter.setField(.albumTitle, "Unsaved title")
        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        #expect(!session.isBusy)
        #expect(session.draft.edit.albumTitle == "Saved title")
        #expect(!session.draft.changed)
    }

    @MainActor
    @Test("Save commits the field that still has focus")
    func saveCommitsFocusedField() async throws {
        var draft = PreviewData.releaseEditDraft(trackCount: 2)
        draft.edit.albumTitle = "Original title"
        let library = PreviewReleaseLibrary(draft: draft)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            draft: draft,
            library: library
        )
        let size = NSSize(width: 900, height: 500)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: session.draft.edit,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                artistSearch: { .fixed([], answers: UnansweredCount()) },
                cover: { EmptyView() },
                audioFacts: { EmptyView() }
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

            #expect(await library.open().edit.albumTitle == "Focused title")
        }
    }

    @MainActor
    @Test(
        "A CUE source used by one track stays in that track's row",
        arguments: [
            ReleaseMetadataTrackColumns.minimumTableWidth, 1_000, 1_400,
        ]
    )
    func individualCueSourcesStayInline(width: CGFloat) async throws {
        let files = try await trackTitleFrames(
            width: width,
            layout: .file,
            shared: false
        )
        let cueFiles = try await trackTitleFrames(
            width: width,
            layout: .cue,
            shared: false
        )
        #expect(cueFiles == files)
    }

    @MainActor
    @Test("Tracks sharing one CUE source keep one caption above their rows")
    func sharedCueSourceKeepsCaption() async throws {
        let files = try await trackTitleFrames(
            width: 1_000,
            layout: .file,
            shared: false
        )
        let shared = try await trackTitleFrames(
            width: 1_000,
            layout: .cue,
            shared: true
        )
        #expect(shared[0].minY > files[0].minY)
        #expect(
            shared[1].minY - shared[0].minY == files[1].minY - files[0].minY
        )
        #expect(shared[0].minX == files[0].minX)
    }

    @MainActor
    private func trackTitleFrames(
        width: CGFloat,
        layout: BridgeSourceAudioLayout,
        shared: Bool
    ) async throws -> [CGRect] {
        var draft = PreviewData.releaseEditDraft(trackCount: 2)
        let contexts = draft.edit.tracks.enumerated()
            .map { index, track in
                PreviewData.releaseEditTrackContext(
                    trackId: track.id,
                    index: index,
                    source: BridgeReleaseEditTrackSource(
                        fileId: shared ? "disc.flac" : "track-\(index).flac",
                        name: shared
                            ? "Disc audio.flac"
                            : "Disc 1/0\(index + 1) A source track with a long filename.flac",
                        layout: layout
                    )
                )
            }
        // Core lays two tracks a sheet carves from one file out as one run,
        // and every other track as a run of its own.
        draft.display.sides = [
            BridgeReleaseEditTrackSide(
                side: .flat,
                headerKey: nil,
                runs: shared
                    ? [.sheet(source: contexts[0].source, tracks: contexts)]
                    : contexts.map { .track(track: $0) }
            )
        ]
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            draft: draft
        )
        let size = NSSize(width: width, height: 700)
        return try await SnapshotTestSupport.withHostedWindow(
            ScrollView {
                ReleaseMetadataEditorContent(
                    session: session,
                    onViewAlbum: { _ in },
                    onPlayTrack: { _ in },
                    onOpenDocument: { _, _ in }
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
            return try draft.edit.tracks.map { track in
                let field = try #require(
                    fields.first { $0.stringValue == track.title }
                )
                return field.convert(field.bounds, to: host)
            }
        }
    }
}

extension ReleaseEditorTests {
    @MainActor
    @Test("cancel restores the saved form")
    func cancelRestoresSavedForm() async throws {
        let draft = PreviewData.releaseEditDraft(trackCount: 2)
        let session = PreviewData.releaseEditSession(
            releaseId: "release-test",
            draft: draft
        )
        await session.fieldWriter.setField(.albumTitle, "Edited title")
        await session.fieldWriter.setPressingFact(
            .area(area: .country(code: "JP"))
        )
        #expect(session.draft.changed)

        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        #expect(!session.isBusy)
        #expect(session.draft.edit.albumTitle == draft.edit.albumTitle)
        #expect(session.draft.edit.pressing.facts == draft.edit.pressing.facts)
        #expect(!session.draft.changed)
    }
}

extension ReleaseEditorTests {
    /// The values core's session returns for each change, in the order
    /// they reached it.
    private actor CoreSession {
        private(set) var titles: [String] = []
        private var draft: BridgeReleaseEditDraft
        private(set) var firstChangeArrived = false
        private var heldFirstChange: CheckedContinuation<Void, Never>?

        init(draft: BridgeReleaseEditDraft) {
            self.draft = draft
        }

        /// Take a title as core does: blank is not savable.
        func setTitle(_ title: String) async -> BridgeReleaseEditDraft {
            // The first change is held until the test lets it go, so a later
            // one would land first if the editor didn't wait for it.
            if !firstChangeArrived {
                firstChangeArrived = true
                Wait.happened()
                await withCheckedContinuation { heldFirstChange = $0 }
            }
            titles.append(title)
            draft.edit.albumTitle = title
            draft.changed = true
            draft.invalid = title.isEmpty ? .emptyAlbumTitle : nil
            return draft
        }

        /// Let the first change take effect.
        func releaseFirstChange() {
            heldFirstChange?.resume()
            heldFirstChange = nil
        }
    }

    @MainActor
    @Test(
        "each change reaches core in the order made, and the editor shows the draft core returns"
    )
    func changesReachCoreInOrder() async throws {
        let draft = PreviewData.releaseEditDraft(trackCount: 1)
        let core = CoreSession(draft: draft)
        let library = PreviewReleaseLibrary(draft: draft)
        let preview = library.opened().session
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            opened: OpenedReleaseEdit(
                draft: draft,
                session: ReleaseEditSessionCommands(
                    draft: preview.draft,
                    setField: { _, value in await core.setTitle(value) },
                    setLabels: preview.setLabels,
                    setBarcodes: preview.setBarcodes,
                    setPressingFact: preview.setPressingFact,
                    setAlbumArtists: preview.setAlbumArtists,
                    setTrack: preview.setTrack,
                    apply: preview.apply,
                    setAlbum: preview.setAlbum,
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
        #expect(session.draft.edit.albumTitle == "Album Title")
        #expect(session.validationMessage == nil)

        await session.fieldWriter.setField(.albumTitle, "")
        #expect(session.draft.changed)
        #expect(session.validationMessage != nil)
    }
}
