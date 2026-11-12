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
                // The design draws the glyph at 14, between the icon roles.
                .font(.system(size: 14, weight: ThemeIcon.medium.weight))
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .themeText(.body)
                .focused(focused)
                .onKeyPress(.escape) {
                    onEscape()
                    return .handled
                }
        }
        .padding(.horizontal, ThemeSpace.group)
        .frame(height: 28)
        // A sunken well like the section control's.
        .background(
            Capsule()
                .fill(Theme.well)
                .overlay(
                    Capsule()
                        .strokeBorder(
                            focused.wrappedValue ? Theme.accent : Color.clear,
                            lineWidth: 1
                        )
                )
        )
        .animation(.easeInOut(duration: 0.15), value: focused.wrappedValue)
    }
}
