import AppKit
import SwiftUI

/// Dismisses the search dropdown on any interaction outside it — without
/// swallowing that interaction. A local event monitor watches mouse-downs and
/// scrolls in the hosting window: events inside `insideRects` (the card, the
/// search field) pass untouched; events outside trigger the dismiss callback
/// and are still returned to AppKit, so the click also lands on whatever was
/// clicked. This replaces a transparent full-window scrim, whose swallowed
/// first click made everything under the dropdown feel dead.
///
/// Mount it as a background of the overlay that hosts the dropdown: the view
/// itself fills that overlay, so event locations convert into the same
/// top-left coordinate space the rects are reported in.
struct SearchDismissMonitor: NSViewRepresentable {
    /// Regions, in the overlay's coordinate space, that interactions may
    /// land in without dismissing.
    let insideRects: [CGRect]
    /// An outside click: the dropdown closes and the field gives up focus.
    let onClickAway: () -> Void
    /// An outside scroll: the dropdown closes; the field keeps focus.
    let onScrollAway: () -> Void

    func makeNSView(context _: Context) -> MonitorView {
        let view = MonitorView()
        apply(to: view)
        return view
    }

    func updateNSView(_ view: MonitorView, context _: Context) {
        apply(to: view)
    }

    private func apply(to view: MonitorView) {
        view.insideRects = insideRects
        view.onClickAway = onClickAway
        view.onScrollAway = onScrollAway
    }

    final class MonitorView: NSView {
        var insideRects: [CGRect] = []
        var onClickAway: (() -> Void)?
        var onScrollAway: (() -> Void)?
        private var monitor: Any?

        // Top-left origin, matching the SwiftUI overlay space the rects are
        // measured in.
        override var isFlipped: Bool { true }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil {
                removeMonitor()
            }
            else if monitor == nil {
                monitor = NSEvent.addLocalMonitorForEvents(
                    matching: [
                        .leftMouseDown, .rightMouseDown, .otherMouseDown,
                        .scrollWheel,
                    ]
                ) { [weak self] event in
                    self?.handle(event) ?? event
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

        private func handle(_ event: NSEvent) -> NSEvent {
            guard let window, event.window === window else {
                return event
            }
            let point = convert(event.locationInWindow, from: nil)
            if insideRects.contains(where: { $0.contains(point) }) {
                return event
            }
            if event.type == .scrollWheel {
                onScrollAway?()
            }
            else {
                onClickAway?()
            }
            return event
        }
    }
}
