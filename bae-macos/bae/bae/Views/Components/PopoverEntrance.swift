import BaeKit
import SwiftUI

/// Animates a popover's content in on mount — a short spring of scale+fade
/// from `anchor` (the edge the popover grows from). Pairs with
/// `PopoverBehavior`, which disables `NSPopover`'s own pop animation: that
/// one runs window-frame updates on the main thread and stutters under any
/// real content build, while this plays over content that is already built.
/// Exit stays instant unless the presenter routes its dismissal through an
/// animated hide first (the queue popover does; hover popovers vanish
/// instantly, like tooltips).
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
    /// Spring the view in when it appears inside a popover whose own
    /// animation `PopoverBehavior` disabled. `anchor` is the edge the
    /// popover visually grows from (where its arrow sits).
    func popoverEntrance(anchor: UnitPoint) -> some View {
        modifier(PopoverEntrance(anchor: anchor))
    }

}

#if DEBUG
    // Hosts a sample popover body with the entrance modifier — it springs from the
    // top anchor on appear and rests at full scale/opacity.
    #Preview("Popover Entrance") {
        VStack(alignment: .leading, spacing: 6) {
            Text("Add to queue")
                .font(.headline)
            Text("Springs in from its anchor when the popover appears.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(width: 240)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 10))
        .popoverEntrance(anchor: .top)
        .padding(48)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
