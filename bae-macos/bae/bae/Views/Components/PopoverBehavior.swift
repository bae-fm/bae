import SwiftUI

/// Configures the `NSPopover` underneath a SwiftUI `.popover` from inside its
/// content (SwiftUI exposes no direct handle; the enclosing window does).
///
/// Always disables the popover's pop in/out animation: its frames run on the
/// main thread alongside the content's first SwiftUI build, and any
/// non-trivial content (the queue's row window, the signal chips' detail
/// tables) starves them into a visible stutter both directions. An instant
/// show/dismiss reads like a menu instead — menus feel crisp precisely
/// because they don't animate.
///
/// `animates = false` also covers content-size updates. That is load-bearing:
/// AppKit animates a presented popover's resize by spinning a nested run loop
/// (`NSMoveHelper _doAnimation`), which can call out to a torn-down run-loop
/// observer and segfault. Every popover whose content can change size while
/// shown must carry this.
///
struct PopoverBehavior: NSViewRepresentable {
    func makeNSView(context _: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window,
                let popover = ObjCExceptionGuard.value(
                    forKey: "_popover",
                    on: window
                ) as? NSPopover
            else {
                return
            }
            popover.animates = false
        }
        return view
    }

    func updateNSView(_: NSView, context _: Context) {}
}

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
