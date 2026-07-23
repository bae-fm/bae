import BaeKit
import SwiftUI

/// Search field in the title bar for searching the library (artists, albums, tracks).
struct LibrarySearchField: View {
    @Binding
    var text: String
    var prompt: LocalizedStringKey
    var focused: FocusState<Bool>.Binding
    var onEscape: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.system(size: 13, weight: .medium))
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 14, weight: .medium))
                .focused(focused)
                .onKeyPress(.escape) {
                    onEscape()
                    return .handled
                }
            if !text.isEmpty {
                Button(action: { text = "" }) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                        .font(.system(size: 13))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 38)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Theme.field)
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .strokeBorder(
                            focused.wrappedValue
                                ? Theme.accent.opacity(0.7)
                                : Color.white.opacity(0.08),
                            lineWidth: 1
                        )
                )
        )
        .animation(.easeInOut(duration: 0.15), value: focused.wrappedValue)
    }
}

#if DEBUG
    // MARK: - Previews

    /// Owns the text and focus state the title bar normally provides — the
    /// "With query" variant seeds text so the clear button shows.
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
