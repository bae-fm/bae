import AppKit
import SwiftUI

/// A window showing one SwiftUI view. Closing it tells its owner, which lets
/// it go, so the view and everything it started end with the window and
/// opening it again builds them anew.
@MainActor
class HostingWindowController: NSWindowController, NSWindowDelegate {
    private let onClose: () -> Void

    /// `content` in a titled window of `contentSize`, or of the content's own
    /// size where that is nil, sized within the limits `sizing` names, with
    /// the window parts `bridging` names managed by the view.
    init<Content: View>(
        content: Content,
        contentSize: CGSize?,
        sizing: NSHostingSizingOptions,
        bridging: NSHostingSceneBridgingOptions,
        onClose: @escaping () -> Void
    ) {
        self.onClose = onClose
        let hosting = NSHostingController(rootView: content)
        hosting.sizingOptions = sizing
        hosting.sceneBridgingOptions = bridging
        let window = NSWindow(contentViewController: hosting)
        if let contentSize {
            window.setContentSize(contentSize)
        }
        window.center()
        // A window restored at launch would open without the primary one.
        window.isRestorable = false
        window.tabbingMode = .disallowed
        // Its owner lets it go once it closes.
        window.isReleasedWhenClosed = false
        super.init(window: window)
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    func windowWillClose(_: Notification) {
        onClose()
    }
}
