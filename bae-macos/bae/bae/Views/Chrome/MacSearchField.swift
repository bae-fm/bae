import BaeKit
import SwiftUI

/// How a `MacSearchField` is drawn.
enum MacSearchFieldKind {
    /// The title bar's rounded field.
    case search
    /// A toolbar's 160-point field, as the Storage Manager's.
    case filter
    /// A field with no box that fills the rest of its row, as the import
    /// list's filter row.
    case inline
}

/// A search or filter field: the magnifier, then the text. A search field
/// rings itself in the accent while it has focus. A disabled field is dimmed.
struct MacSearchField: View {
    var kind: MacSearchFieldKind = .search
    @Binding
    var text: String
    var prompt: LocalizedStringKey
    var focused: FocusState<Bool>.Binding
    var onEscape: () -> Void = {}

    @Environment(\.isEnabled)
    private var isEnabled

    var body: some View {
        HStack(
            spacing: kind == .filter ? ThemeSpace.compact : ThemeSpace.related
        ) {
            glyph
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .themeText(.body)
                .focused(focused)
                .accessibilityLabel(prompt)
                .accessibilityAddTraits(.isSearchField)
                .onKeyPress(.escape) {
                    onEscape()
                    return .handled
                }
        }
        .modifier(MacSearchFieldBox(kind: kind, focused: focused.wrappedValue))
        .opacity(isEnabled ? 1 : ThemeOpacity.off)
    }

    @ViewBuilder
    private var glyph: some View {
        switch kind {
        case .search:
            Image(systemName: "magnifyingglass")
                .foregroundStyle(Theme.textTertiary)
                .accessibilityHidden(true)
                // The design draws the glyph at 14, between the icon roles.
                .font(.system(size: 14, weight: ThemeIcon.medium.weight))
        case .filter:
            Image(systemName: "magnifyingglass")
                .themeIcon(.small)
                .foregroundStyle(Theme.macTextSecondary)
                .accessibilityHidden(true)
        case .inline:
            // The field's name says what it does, so the glyph says
            // nothing more to assistive technology.
            Image(systemName: "magnifyingglass")
                .themeIcon(.medium)
                .foregroundStyle(Theme.textTertiary)
                .accessibilityHidden(true)
        }
    }
}

/// The box a `MacSearchField`'s kind draws round its glyph and text.
private struct MacSearchFieldBox: ViewModifier {
    let kind: MacSearchFieldKind
    let focused: Bool

    func body(content: Content) -> some View {
        switch kind {
        case .search:
            content
                .padding(.horizontal, ThemeSpace.group)
                .frame(height: 28)
                // A sunken well like the section control's.
                .background(
                    Capsule()
                        .fill(Theme.well)
                        .overlay(
                            Capsule()
                                .strokeBorder(
                                    focused ? Theme.accent : Color.clear,
                                    lineWidth: 1
                                )
                        )
                )
                .animation(.easeInOut(duration: 0.15), value: focused)
        case .filter:
            content
                .padding(.horizontal, ThemeSpace.related)
                .frame(width: 160, height: 24, alignment: .leading)
                .background(
                    Theme.field,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.field)
                )
        case .inline:
            content
        }
    }
}
