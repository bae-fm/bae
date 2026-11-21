import SwiftUI

/// The piece every macOS row with a leading icon is built from: `lead` (an
/// icon, a status glyph, a progress mark), the content, then `trail` (a size,
/// a time, a cancel mark). MacItem draws `trail` in the secondary foreground
/// style. The leading column is
/// `ThemeSize.lead` wide and as tall as the first line, the icon centred in
/// it; the content starts
/// `ThemeSpace.related` after it. The column stays when `lead` is empty, so a
/// row with no icon keeps the same text edge as its neighbours. It has no
/// inset of its own: the row it sits in places it.
///
/// `centered` sets the lead and the trail at the middle of the content's
/// lines rather than on its first line (the design's `centerLead` and
/// `centerTrail` together), and `wide` starts the content
/// `ThemeSpace.group - ThemeSpace.line` after the leading column.
public struct MacItem<Lead: View, Content: View, Trail: View>: View {
    private let centered: Bool
    private let wide: Bool
    private let lead: Lead
    private let content: Content
    private let trail: Trail

    public init(
        centered: Bool = false,
        wide: Bool = false,
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder content: () -> Content,
        @ViewBuilder trail: () -> Trail
    ) {
        self.centered = centered
        self.wide = wide
        self.lead = lead()
        self.content = content()
        self.trail = trail()
    }

    public var body: some View {
        // The three start at the item's top, as the design system lays them
        // out: a control at the end, taller than a line, sits from the top
        // of the first line down rather than pushing the row taller. A
        // centred item sets the lead and the end at the middle of the
        // content's lines instead.
        HStack(
            alignment: centered ? .center : .top,
            spacing: wide
                ? ThemeSpace.group - ThemeSpace.line : ThemeSpace.related
        ) {
            // The hidden line gives the column the first line's height, and
            // the lead centres on that line over it.
            Text(verbatim: " ").themeText(.body).hidden()
                .frame(width: ThemeSize.lead)
                .overlay { lead }
            content.frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: ThemeSpace.related) { trail }
                .foregroundStyle(Theme.macTextSecondary)
        }
    }
}

extension MacItem where Trail == EmptyView {
    public init(
        @ViewBuilder lead: () -> Lead,
        @ViewBuilder content: () -> Content
    ) {
        self.init(lead: lead, content: content, trail: { EmptyView() })
    }
}
