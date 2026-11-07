import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// A sheet whose footer is a secondary button then the primary one.
    static let sheetFooterScenes: [ShotScene] = [
        ShotScene(
            id: "sheet-footer-rename-library",
            size: CGSize(width: 420, height: 240)
        ) {
            ShotStage(RenameLibrarySheetStaged())
        }
    ]
}

private struct RenameLibrarySheetStaged: View {
    @State
    private var state = RenameLibrarySheetState(
        id: "library-preview",
        newName: "Home"
    )

    var body: some View {
        RenameLibrarySheet(state: $state, onCancel: {}, onCommit: { _ in })
    }
}
