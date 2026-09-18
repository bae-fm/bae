import AppKit
import SwiftUI

/// Closes an overlay drawn inside the window — a card anchored under its
/// trigger rather than an `NSPopover` — the two ways a person closes one: a
/// click anywhere outside it, and Escape.
///
/// A popover gets both from AppKit. An overlay is an ordinary view in the
/// window's tree, so nothing closes it for us: a local monitor watches every
/// mouse-down in the app and every key-down, mounted only while the overlay
/// is up. The click still lands on whatever was clicked — a person closing
/// the card by clicking a track expects the track to play — and Escape is
/// swallowed, since it was aimed at the card.
struct OverlayDismissMonitor: NSViewRepresentable {
    /// The view that opens the overlay. A click on it is the trigger's own
    /// toggle, not a click away: closing on it too would close and reopen
    /// the overlay in one click.
    let trigger: OverlayAnchor
    let dismiss: () -> Void

    func makeNSView(context _: Context) -> MonitorView {
        MonitorView(trigger: trigger, dismiss: dismiss)
    }

    func updateNSView(_ view: MonitorView, context _: Context) {
        view.trigger = trigger
        view.dismiss = dismiss
    }

    final class MonitorView: NSView {
        var trigger: OverlayAnchor
        var dismiss: () -> Void
        private var monitor: Any?

        init(trigger: OverlayAnchor, dismiss: @escaping () -> Void) {
            self.trigger = trigger
            self.dismiss = dismiss
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("not loaded from a nib")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil {
                removeMonitor()
            }
            else if monitor == nil {
                monitor = NSEvent.addLocalMonitorForEvents(
                    matching: [
                        .leftMouseDown, .rightMouseDown, .otherMouseDown,
                        .keyDown,
                    ]
                ) { [weak self] event in
                    self?.handle(event)
                }
            }
        }

        deinit {
            removeMonitor()
        }

        private func removeMonitor() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
                self.monitor = nil
            }
        }

        private static let escapeKeyCode: UInt16 = 53

        /// The event, or `nil` to swallow it.
        private func handle(_ event: NSEvent) -> NSEvent? {
            if event.type == .keyDown {
                guard event.keyCode == Self.escapeKeyCode else { return event }
                dismiss()
                return nil
            }
            // A click in another window is outside the overlay by definition;
            // a click in this one is outside when it lands neither on the
            // overlay nor on the trigger that opened it.
            guard let window, event.window === window else {
                dismiss()
                return event
            }
            let point = event.locationInWindow
            let inOverlay = convert(bounds, to: nil).contains(point)
            if !inOverlay, !trigger.contains(windowPoint: point) {
                dismiss()
            }
            return event
        }
    }
}

/// Where an overlay's trigger is on screen, as the AppKit view under it —
/// the one coordinate space a mouse-down's location can be checked against
/// without guessing how the SwiftUI tree sits in the window.
@MainActor
final class OverlayAnchor {
    fileprivate weak var view: NSView?

    func contains(windowPoint: NSPoint) -> Bool {
        guard let view, view.window != nil else { return false }
        return view.convert(view.bounds, to: nil).contains(windowPoint)
    }
}

/// Marks the trigger's place in the window: mounted behind the trigger, it is
/// sized to it and hands the overlay's anchor the AppKit view to measure.
struct OverlayTrigger: NSViewRepresentable {
    let anchor: OverlayAnchor

    func makeNSView(context _: Context) -> NSView {
        let view = NSView()
        anchor.view = view
        return view
    }

    func updateNSView(_ view: NSView, context _: Context) {
        anchor.view = view
    }
}
