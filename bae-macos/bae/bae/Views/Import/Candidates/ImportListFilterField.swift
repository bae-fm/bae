import BaeKit
import SwiftUI

/// The list's filter field: the typed text, and while the text or the filter
/// entry narrows the list, how many of the tab's rows it shows.
struct ImportListFilterField: View {
    @Binding
    var text: String
    var focused: FocusState<Bool>.Binding
    /// The count for the filter the field holds now; `nil` while nothing
    /// narrows the list, and while core has not counted under this filter.
    let narrowed: BridgeNarrowedCount?

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "magnifyingglass")
                .themeIcon(ImportFilterBarLayout.glyph)
                .foregroundStyle(.tertiary)
            TextField("Filter…", text: $text)
                .textFieldStyle(.plain)
                .themeText(.body)
                .focused(focused)
            if let narrowed {
                Text(
                    "\(narrowed.shown.formatted()) of \(narrowed.total.formatted())"
                )
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .fixedSize()
            }
            if !text.isEmpty {
                Button {
                    text = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .themeIcon(ImportFilterBarLayout.glyph)
                        .foregroundStyle(.tertiary)
                        .filterBarControl()
                }
                .buttonStyle(.accentPlain)
            }
        }
    }
}
