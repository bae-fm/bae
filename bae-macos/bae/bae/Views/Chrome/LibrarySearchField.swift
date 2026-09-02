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
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.tertiary)
                .font(.system(size: 13, weight: .semibold))
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13.5, weight: .medium))
                .focused(focused)
                .onKeyPress(.escape) {
                    onEscape()
                    return .handled
                }
            if !text.isEmpty {
                Button(action: { text = "" }) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 20, height: 20)
                        .background(
                            Circle().fill(Color.white.opacity(0.12))
                        )
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        // A sunken well like the section control's; the accent edge appears
        // only while the field has focus.
        .background(
            RoundedRectangle(cornerRadius: 9)
                .fill(TitleBarChrome.well)
                .overlay(
                    RoundedRectangle(cornerRadius: 9)
                        .strokeBorder(
                            Theme.accent.opacity(
                                focused.wrappedValue ? 0.6 : 0
                            ),
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
