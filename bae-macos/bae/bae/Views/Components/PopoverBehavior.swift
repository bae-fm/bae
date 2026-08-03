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
