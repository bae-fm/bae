import AppKit
import BaeKit
import SwiftUI

// MARK: - Candidate selection

extension ImportView {
    /// The sidebar's selection, which core holds: the list shows the loaded
    /// rows core marks selected, and what a person does to them goes back to
    /// core as the change it is.
    var candidateSelectionBinding: Binding<Set<String>> {
        Binding(
            get: { importStore.selectedLoadedKeys },
            set: { keys in
                listSlot.changeSelection(
                    to: keys,
                    from: importStore.selectedLoadedKeys,
                    by: SelectionGesture(NSEvent.modifierFlags)
                )
            },
        )
    }
}

extension SelectionGesture {
    /// The gesture the keys held down make of a change to the list's rows.
    init(_ modifiers: NSEvent.ModifierFlags) {
        if modifiers.contains(.command) {
            self = .toggle
        }
        else if modifiers.contains(.shift) {
            self = .extend
        }
        else {
            self = .replace
        }
    }
}
