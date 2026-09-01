import AppKit
import SwiftUI

/// Ends the active text-field edit on a click outside it — without swallowing
/// the click.
///
/// AppKit moves first responder only when a click lands on another view that
/// accepts it; a click on empty space, a label, or plain chrome leaves the
/// caret and the field's focus chrome stranded in the last field. A local
/// monitor watches every mouse-down in the app: when the clicked window's
/// first responder is a text field's editor and the click is outside both the
/// editor and its field, the field resigns — committing its edit the way any
/// blur does — and the event still lands on whatever was clicked.
///
/// Mounted once at the root of the window's content. The monitor reads the
/// clicked window off each event, so popovers and sheets get the same
/// behaviour against their own fields.
struct FieldClickAwayMonitor: NSViewRepresentable {
    func makeNSView(context _: Context) -> MonitorView { MonitorView() }
    func updateNSView(_: MonitorView, context _: Context) {}

    final class MonitorView: NSView {
        private var monitor: Any?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window == nil {
                removeMonitor()
            }
            else if monitor == nil {
                monitor = NSEvent.addLocalMonitorForEvents(
                    matching: [
                        .leftMouseDown, .rightMouseDown, .otherMouseDown,
                    ]
                ) { event in
                    Self.resignFieldOnOutsideClick(event)
                    return event
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

        /// Resign the clicked window's focused text field when the click lands
        /// outside it. Only a field editor resigns: a non-field text view (a
        /// selectable document) keeps its responder status, since blurring it
        /// would break selection in progress.
        private static func resignFieldOnOutsideClick(_ event: NSEvent) {
            guard let window = event.window,
                let editor = window.firstResponder as? NSTextView,
                editor.isFieldEditor,
                let content = window.contentView
            else { return }
            // `hitTest` takes superview coordinates; the content view's
            // superview is the window-sized frame view, so window coordinates
            // are that space.
            let hit = content.hitTest(event.locationInWindow)
            let field = editor.delegate as? NSView
            let insideField =
                hit.map { clicked in
                    clicked.isDescendant(of: editor)
                        || field.map { clicked.isDescendant(of: $0) } == true
                } ?? false
            if !insideField {
                window.makeFirstResponder(nil)
            }
        }
    }
}
