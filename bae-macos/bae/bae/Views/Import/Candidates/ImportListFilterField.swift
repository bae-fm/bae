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
    /// The field shows nothing while core has not counted the rows.
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
                narrowedText(
                    shown: narrowed.shown.formatted(),
                    total: narrowed.total.formatted()
                )
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

    private func narrowedText(shown: String, total: String) -> some View {
        Text("\(shown) of \(total)")
            .themeText(.detail)
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .fixedSize()
    }
}
