import SwiftUI

/// The piece every macOS row with a leading icon is built from: `lead` (an
/// icon, a status glyph, a progress mark), the content, then `trail` (a size,
/// a time, a cancel mark). The leading column is `ThemeSize.lead` wide and as
/// tall as the first line, the icon centred in it; the content starts
/// `ThemeSpace.related` after it. The column stays when `lead` is empty, so a
/// row with no icon keeps the same text edge as its neighbours. It has no
/// inset of its own: the row it sits in places it.
public struct RowItem<Lead: View, Content: View, Trail: View>: View {
    private let lead: Lead
    private let content: Content
    private let trail: Trail

    public init(
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder content: () -> Content,
        @ViewBuilder trail: () -> Trail
    ) {
        self.lead = lead()
        self.content = content()
        self.trail = trail()
    }

    public var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
            // The hidden line gives the column the first line's height and
            // baseline, and the lead centres on that line over it. As an
            // overlay the lead adds no baseline of its own: in a stack beside
            // the line, a native view's baseline at its top would become the
            // column's, and the row would align its text to the lead's top.
            Text(verbatim: " ").themeText(.body).hidden()
                .frame(width: ThemeSize.lead)
                .overlay { lead }
            content.frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: ThemeSpace.related) { trail }
        }
    }
}

extension RowItem where Trail == EmptyView {
    public init(
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder content: () -> Content
    ) {
        self.init(lead: lead, content: content, trail: { EmptyView() })
    }
}
