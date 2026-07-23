import SwiftUI

extension View {
    /// The edit sheet's card padding: how far every grouped-form row's
    /// content sits from the card's edges, on top of the platform's own row
    /// insets. (`listRowInsets` is ignored by macOS grouped forms, so this
    /// pads the row content itself.)
    func presetEditorRowInsets() -> some View {
        padding(EdgeInsets(top: 2, leading: 8, bottom: 0, trailing: 8))
    }
}
