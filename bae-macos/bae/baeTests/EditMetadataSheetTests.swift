import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Edit metadata sheet")
struct EditMetadataSheetTests {
    @MainActor
    @Test("Manual metadata omits Reset to Source")
    func manualMetadataOmitsReset() {
        #expect(!sheet(canResetToSource: false).resetButtonIsVisible)
    }

    @MainActor
    @Test("source-backed metadata offers Reset to Source")
    func sourceBackedMetadataOffersReset() {
        #expect(sheet(canResetToSource: true).resetButtonIsVisible)
    }

    @MainActor
    private func sheet(canResetToSource: Bool) -> EditMetadataSheet {
        let seed = BridgeReleaseEditSeed(
            edit: PreviewData.editMetadataSeed(trackCount: 2),
            canResetToSource: canResetToSource
        )
        return EditMetadataSheet(
            seed: seed,
            onSave: { _ in },
            onReset: { seed.edit },
            onCancel: {}
        )
    }
}
