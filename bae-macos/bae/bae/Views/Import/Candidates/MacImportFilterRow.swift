import BaeKit
import SwiftUI

/// The import list's filter row: the filter field, then the status controls
/// the caller puts at its end. A click anywhere on the row puts the caret in
/// the field.
struct MacImportFilterRow<Trailing: View>: View {
    @Binding
    var filterText: String
    /// Reads the count for the filter the field holds now; `nil` while
    /// nothing narrows the list, and while core has not counted under this
    /// filter. The filter field reads it, so a new count redraws only the
    /// field.
    let narrowed: () -> BridgeNarrowedCount?
    @ViewBuilder
    let trailing: () -> Trailing

    @FocusState
    private var filterFocused: Bool

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            ImportListFilterField(
                text: $filterText,
                focused: $filterFocused,
                narrowed: narrowed
            )
            trailing()
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: ImportFilterBarLayout.rowHeight)
        // The controls at the end keep their own clicks.
        .contentShape(Rectangle())
        .onTapGesture { filterFocused = true }
    }
}
