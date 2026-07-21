import SwiftUI

extension View {
    /// Feeds this scrollable view's offset and scroll phase into the
    /// environment `HeaderCollapse`, identified by `id` so the model can
    /// tell pane hand-offs apart. Apply directly to the `ScrollView` or
    /// `List` being observed. No-op when no `HeaderCollapse` is in the
    /// environment (previews, contexts without a collapsing header).
    func reportsHeaderScroll(id: String) -> some View {
        modifier(HeaderScrollReporter(id: id))
    }
}

private struct HeaderScrollReporter: ViewModifier {
    let id: String

    @Environment(HeaderCollapse.self)
    private var headerCollapse: HeaderCollapse?

    func body(content: Content) -> some View {
        content
            .onScrollGeometryChange(for: Double.self) { geometry in
                // Clamp to the scrollable span: rubber-band overscroll is not
                // scrolling, and the elastic snap-back at the bottom edge must
                // not register as upward travel (it would trip the model's
                // expand hysteresis on every bounce).
                let offset =
                    geometry.contentOffset.y + geometry.contentInsets.top
                let span =
                    geometry.contentSize.height + geometry.contentInsets.top
                    + geometry.contentInsets.bottom
                    - geometry.containerSize.height
                return min(max(offset, 0), max(span, 0))
            } action: { _, offset in
                headerCollapse?.reportScroll(scroller: id, offset: offset)
            }
            .onScrollPhaseChange { _, newPhase in
                headerCollapse?
                    .reportPhase(
                        scroller: id,
                        isScrolling: newPhase != .idle
                    )
            }
    }
}
