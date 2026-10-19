import SwiftUI

/// Keeps a hover-triggered popover presented while the pointer crosses the gap
/// between its trigger and its content.
struct HoverPopoverModifier<PopoverContent: View>: ViewModifier {
    let arrowEdge: Edge
    let popoverContent: () -> PopoverContent

    @State
    private var isShowing = false
    @State
    private var hoverTask: DispatchWorkItem?

    func body(content: Content) -> some View {
        content
            .onHover { scheduleHover(show: $0) }
            .popover(isPresented: $isShowing, arrowEdge: arrowEdge) {
                popoverContent()
                    .onHover { hovering in
                        if hovering {
                            hoverTask?.cancel()
                        }
                        else {
                            scheduleHover(show: false)
                        }
                    }
            }
            .onDisappear {
                hoverTask?.cancel()
            }
    }

    private func scheduleHover(show: Bool) {
        hoverTask?.cancel()
        let task = DispatchWorkItem {
            isShowing = show
        }
        hoverTask = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: task)
    }
}

extension View {
    func hoverPopover<PopoverContent: View>(
        arrowEdge: Edge,
        @ViewBuilder content: @escaping () -> PopoverContent
    ) -> some View {
        modifier(
            HoverPopoverModifier(
                arrowEdge: arrowEdge,
                popoverContent: content
            )
        )
    }
}
