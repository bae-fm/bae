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
    func persistedReleaseSessionSavesAndAdvancesItsCancelPoint() async {
        let recorder = SavedEditRecorder()
        let seed = PreviewData.releaseEditSeed(trackCount: 2)
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { releaseId, edit in
                await recorder.record(releaseId, edit)
            },
            reset: { _ in seed.edit }
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
        while session.isBusy {
            await Task.yield()
        }
        #expect(session.form.albumTitle == "Saved title")
        #expect(!session.hasChanges)
    }

    @MainActor
    @Test("Save commits the field that still has focus")
    func saveCommitsFocusedField() async throws {
        let recorder = SavedEditRecorder()
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        seed.edit.albumTitle = "Original title"
        let resetEdit = seed.edit
        let session = ReleaseMetadataEditSession(
            releaseId: "release-test",
            seed: seed,
            save: { releaseId, edit in
                await recorder.record(releaseId, edit)
            },
            reset: { _ in resetEdit }
        )
        let size = NSSize(width: 900, height: 500)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ReleaseMetadataHeader(
                values: session.form,
                writer: session.fieldWriter,
                editingCommands: session.editingCommands,
                cover: { EmptyView() },
                context: { EmptyView() },
                sourceAudio: { EmptyView() }
            )
            .environment(Library.stub())
            .environment(UiStore())
            .frame(width: size.width, height: size.height),
            size: size
        )
        await SnapshotTestSupport.settle(host)

        let titleField = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .first { $0.stringValue == "Original title" }
        )
        #expect(window.makeFirstResponder(titleField))
        titleField.stringValue = "Focused title"
        titleField.delegate?.controlTextDidChange?(
            Notification(
                name: NSControl.textDidChangeNotification,
                object: titleField
            )
        )

        await withCheckedContinuation { continuation in
            session.save { continuation.resume() }
        }

        #expect(await recorder.recorded().1?.albumTitle == "Focused title")
        window.contentView = nil
        window.orderOut(nil)
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
    private func sheet(canResetToSource: Bool) -> EditMetadataSheet {
        var seed = PreviewData.releaseEditSeed(trackCount: 2)
        seed.canResetToSource = canResetToSource
        return EditMetadataSheet(
            releaseId: "release-test",
            seed: seed,
            onSave: { _ in },
            onReset: { seed.edit },
            onSaved: {},
            onCancel: {}
        )
    }
}
