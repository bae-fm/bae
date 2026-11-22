import SwiftUI

/// The solid accent selection for a choice row or Settings pane.
private struct MacSelection<Outline: Shape>: ViewModifier {
    let selected: Bool
    let outline: Outline

    @Environment(\.accentChoice)
    private var accent

    func body(content: Content) -> some View {
        content
            .foregroundStyle(selected ? Theme.onFill : Theme.textPrimary)
            .environment(\.chipsOnFill, selected)
            .background(
                selected ? accent.buttonColor : Color.clear,
                in: outline
            )
    }
}

extension View {
    /// Draws a solid accent selection within the caller's outline.
    public func macSelection(
        _ selected: Bool,
        in outline: some Shape
    ) -> some View {
        modifier(MacSelection(selected: selected, outline: outline))
    }
}
