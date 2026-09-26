import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Edit metadata sheet")
struct EditMetadataSheetTests {
    private actor SavedEditRecorder {
        private(set) var releaseId: String?
        private(set) var edit: BridgeReleaseUserEdit?

        func record(_ releaseId: String, _ edit: BridgeReleaseUserEdit) {
            self.releaseId = releaseId
            self.edit = edit
        }

        func recorded() -> (String?, BridgeReleaseUserEdit?) {
            (releaseId, edit)
        }
    }

    @Test("Modal size follows the host with minimum and maximum bounds")
    func modalSize() {
        #expect(
            EditMetadataSheet.modalSize(
                in: CGSize(width: 1_600, height: 1_000)
            ) == CGSize(width: 1_200, height: 860)
        )
        #expect(
            EditMetadataSheet.modalSize(
                in: CGSize(width: 820, height: 650)
            ) == CGSize(width: 760, height: 600)
        )
        #expect(
            EditMetadataSheet.modalSize(
                in: CGSize(width: 700, height: 500)
            ) == CGSize(width: 700, height: 500)
        )
    }

    @MainActor
    @Test("Done and Library save through one persisted release session")
    func persistedReleaseSessionSavesAndAdvancesItsCancelPoint() async throws {
        let recorder = SavedEditRecorder()
        let seed = PreviewData.releaseEditSeed(trackCount: 2)
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { releaseId, edit in
                await recorder.record(releaseId, edit)
            },
            reset: { _ in
                seed.edit
            }
        )
        await session.fieldWriter.setField(.albumTitle, "Saved title")

        await withCheckedContinuation { continuation in
            session.save { continuation.resume() }
        }

        let recorded = await recorder.recorded()
        #expect(recorded.0 == "release-test")
        #expect(recorded.1?.albumTitle == "Saved title")
        #expect(!session.hasChanges)

        await session.fieldWriter.setField(.albumTitle, "Unsaved title")
        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        #expect(!session.isBusy)
        #expect(session.form.albumTitle == "Saved title")
        #expect(!session.hasChanges)
    }

    @MainActor
    @Test("Save commits the field that still has focus")
    func saveCommitsFocusedField() async throws {
        let recorder = SavedEditRecorder()
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        seed.edit.albumTitle = "Original title"
        let reset = seed.edit
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { releaseId, edit in
                await recorder.record(releaseId, edit)
            },
            reset: { _ in reset }
        )
        let size = NSSize(width: 900, height: 500)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseMetadataHeader(
                values: session.form,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                cover: { EmptyView() },
                audioFacts: { EmptyView() }
            )
            .environment(Library.stub())
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
                session.save { continuation.resume() }
            }

            #expect(await recorder.recorded().1?.albumTitle == "Focused title")
        }
    }

    @MainActor
    @Test("Direct-entry metadata omits Reset to Source")
    func directEntryMetadataOmitsReset() {
        #expect(!sheet(canResetToSource: false).resetButtonIsVisible)
    }

    @MainActor
    @Test("source-backed metadata offers Reset to Source")
    func sourceBackedMetadataOffersReset() {
        #expect(sheet(canResetToSource: true).resetButtonIsVisible)
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
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        for index in seed.display.tracks.indices {
            seed.display.tracks[index].sources = [
                BridgeReleaseEditTrackSource(
                    fileId: shared ? "disc.flac" : "track-\(index).flac",
                    name: shared
                        ? "Disc audio.flac"
                        : "Disc 1/0\(index + 1) A source track with a long filename.flac",
                    layout: layout
                )
            ]
        }
        let reset = seed.edit
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { _, _ in },
            reset: { _ in reset }
        )
        let size = NSSize(width: width, height: 700)
        return try await SnapshotTestSupport.withHostedWindow(
            ScrollView {
                ReleaseMetadataEditorContent(
                    session: session,
                    onPlayTrack: { _ in }
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
            return try seed.edit.tracks.map { track in
                let field = try #require(
                    fields.first { $0.stringValue == track.title }
                )
                return field.convert(field.bounds, to: host)
            }
        }
    }

    @MainActor
    private func sheet(canResetToSource: Bool) -> EditMetadataSheet {
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        seed.canResetToSource = canResetToSource
        return EditMetadataSheet(
            releaseId: "release-test",
            seed: seed,
            onSave: { _ in },
            onReset: {
                seed.edit
            },
            onSaved: {},
            onCancel: {}
        )
    }
}

extension EditMetadataSheetTests {
    @MainActor
    @Test("Reset replaces edited values and cancel restores the saved form")
    func resetReplacesEditsAndCancelRestoresSavedForm() async throws {
        let seed = PreviewData.releaseEditSeed(trackCount: 2)
        var source = seed.edit
        source.albumTitle = "Source title"
        source.pressing.country = ""
        let sourceForm = source
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { _, _ in },
            reset: { _ in sourceForm }
        )
        await session.fieldWriter.setField(.albumTitle, "Edited title")
        await session.fieldWriter.setField(.country, "JP")

        session.resetToSource()
        try await Wait.until { !session.isBusy }
        #expect(!session.isBusy)
        #expect(session.form.albumTitle == "Source title")
        #expect(session.form.pressing.country.isEmpty)
        #expect(session.hasChanges)

        session.cancelChanges()
        try await Wait.until { !session.isBusy }
        #expect(!session.isBusy)
        #expect(session.form.albumTitle == seed.edit.albumTitle)
        #expect(session.form.pressing.country == seed.edit.pressing.country)
        #expect(!session.hasChanges)
    }
}
