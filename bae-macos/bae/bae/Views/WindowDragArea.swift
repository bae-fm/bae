import SwiftUI

/// Makes an area behave like a native title bar: draggable to move the window,
/// double-click to zoom (maximize/restore). Needed because .hiddenTitleBar removes
/// the system title bar and our custom HStack doesn't inherit that behavior.
struct WindowDragArea: NSViewRepresentable {
    func makeNSView(context _: Context) -> NSView {
        DragView()
    }

    func updateNSView(_: NSView, context _: Context) {}

    private class DragView: NSView {
        override func mouseDown(with event: NSEvent) {
            if event.clickCount == 2 {
                window?.zoom(nil)
            }
            else {
                window?.performDrag(with: event)
            }
        }
    }
}
