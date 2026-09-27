import BaeKit
import SwiftUI

/// Springs a popover's content in from `anchor` on appear, in place of the
/// `NSPopover` animation that `PopoverBehavior` turns off because it stutters.
struct PopoverEntrance: ViewModifier {
    let anchor: UnitPoint

    @State
    private var shown = false

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .scaleEffect(shown ? 1 : 0.96, anchor: anchor)
            .onAppear {
                withAnimation(.spring(duration: 0.22, bounce: 0.15)) {
                    shown = true
                }
            }
            .onDisappear {
                shown = false
            }
    }
}

extension View {
    /// Springs the view in from `anchor`, the edge where the popover's arrow
    /// sits.
    func popoverEntrance(anchor: UnitPoint) -> some View {
        modifier(PopoverEntrance(anchor: anchor))
    }

}

#if DEBUG
    #Preview("Popover Entrance") {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            Text(verbatim: "Add to queue")
                .themeText(.heading)
            Text(
                verbatim: "Springs in from its anchor when the popover appears."
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
        }
        .padding(ThemeSpace.edge)
        .frame(width: 240)
        .background(
            Theme.surface,
            in: RoundedRectangle(cornerRadius: ThemeRadius.card)
        )
        .popoverEntrance(anchor: .top)
        .padding(ThemeSpace.page)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
