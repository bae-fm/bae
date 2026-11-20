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
                albumArtistStanding: release.draft.albumArtistStanding,
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
        let release = twoTrackRelease(layout: layout, shared: shared)
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
                ReleaseMetadataEditorContent(
                    session: session,
                    draft: release.draft,
                    lead: { EmptyView() },
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
    private func twoTrackRelease(
        layout: BridgeSourceAudioLayout,
        shared: Bool
    ) -> PreviewReleaseEdit {
        var release = PreviewData.releaseEdit(trackCount: 2)
        let rows = release.values.tracks.enumerated()
            .map { index, track in
                BridgeReleaseEditTrackRow(
                    edit: track,
                    context: PreviewData.releaseEditTrackContext(
                        trackId: track.id,
                        index: index,
                        source: BridgeReleaseEditTrackSource(
                            fileId: shared
                                ? "disc.flac" : "track-\(index).flac",
                            name: shared
                                ? "Disc audio.flac"
                                : "Disc 1/0\(index + 1) A source track with a long filename.flac",
                            layout: layout
                        )
                    ),
                    artistStanding: nil
                )
            }
        // Core lays two tracks a sheet carves from one file out as one run,
        // and every other track as a run of its own.
        release.draft.sides = [
            BridgeReleaseEditTrackSide(
                side: .flat,
                headerKey: nil,
                runs: shared
                    ? [.sheet(source: rows[0].context.source, rows: rows)]
                    : rows.map { .track(row: $0) }
            )
        ]
        return release
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
                    setAlbum: preview.setAlbum,
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
