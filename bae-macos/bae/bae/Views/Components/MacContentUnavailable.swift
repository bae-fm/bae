import SwiftUI

/// The message of a pane with nothing to show: a title, and a line under it
/// where there is something to add, centred in the space the pane has.
struct MacContentUnavailable: View {
    let title: LocalizedStringKey
    var description: LocalizedStringKey?

    var body: some View {
        if let description {
            ContentUnavailableView {
                Text(title)
            } description: {
                Text(description)
            }
        }
        else {
            ContentUnavailableView { Text(title) }
        }
    }
}
