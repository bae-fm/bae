import BaeKit
import SwiftUI

/// The list's filter field: the typed text, and while the text or the filter
/// entry narrows the list, how many of the tab's rows it shows.
struct ImportListFilterField: View {
    @Binding
    var text: String
    var focused: FocusState<Bool>.Binding
    /// Reads the count for the filter the field holds now; `nil` while
    /// nothing narrows the list, and while core has not counted under this
    /// filter. The field shows nothing in place of the count while core
    /// counts.
    let narrowed: () -> BridgeNarrowedCount?

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            // The field's name says what it does, so the glyph says
            // nothing more to assistive technology.
            Image(systemName: "magnifyingglass")
                .themeIcon(ImportFilterBarLayout.glyph)
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
            TextField("Filter", text: $text, prompt: Text("Filter"))
                .textFieldStyle(.plain)
                .themeText(.body)
                .focused(focused)
            if let narrowed = narrowed() {
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
