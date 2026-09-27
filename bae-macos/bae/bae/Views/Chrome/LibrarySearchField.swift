import BaeKit
import SwiftUI

/// The title bar's field for searching artists, albums and tracks.
struct LibrarySearchField: View {
    @Binding
    var text: String
    var prompt: LocalizedStringKey
    var focused: FocusState<Bool>.Binding
    var onEscape: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.tertiary)
                .themeIcon(.medium)
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .themeText(.body)
                .focused(focused)
                .onKeyPress(.escape) {
                    onEscape()
                    return .handled
                }
            if !text.isEmpty {
                Button(action: { text = "" }) {
                    Image(systemName: "xmark")
                        .themeIcon(.badge)
                        .foregroundStyle(.secondary)
                        .frame(width: 20, height: 20)
                        .background(
                            Circle().fill(Theme.hover)
                        )
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: 36)
        // A sunken well like the section control's.
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Theme.well)
                .overlay(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .strokeBorder(
                            focused.wrappedValue ? Theme.accent : Color.clear,
                            lineWidth: 1
                        )
                )
        )
        .animation(.easeInOut(duration: 0.15), value: focused.wrappedValue)
    }
}

#if DEBUG
    // MARK: - Previews

    /// Owns the text and focus state the title bar normally provides.
    private struct LibrarySearchFieldPreview: View {
        @State
        var text: String
        @FocusState
        private var focused: Bool

        var body: some View {
            LibrarySearchField(
                text: $text,
                prompt: "Search",
                focused: $focused,
                onEscape: {},
            )
            .frame(width: 300)
            .padding()
            .background(Theme.surface)
        }
    }

    #Preview("Empty") {
        LibrarySearchFieldPreview(text: "")
    }

    #Preview("With query") {
        LibrarySearchFieldPreview(text: "Artist Name")
    }
#endif
