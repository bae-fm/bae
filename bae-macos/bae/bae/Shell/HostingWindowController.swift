import AppKit
import SwiftUI

/// A window showing one SwiftUI view. Closing it tells its owner, which lets
/// it go, so the view and everything it started end with the window and
/// opening it again builds them anew.
@MainActor
class HostingWindowController: NSWindowController, NSWindowDelegate {
    private let frameAutosave: WindowFrameAutosave?
    private let onClose: () -> Void

    /// `content` in a titled window of `contentSize`, or of the content's own
    /// size where that is nil, sized within the limits `sizing` names, with
    /// the window parts `bridging` names managed by the view.
    init<Content: View>(
        content: Content,
        contentSize: CGSize?,
        sizing: NSHostingSizingOptions,
        bridging: NSHostingSceneBridgingOptions,
        frameAutosave: WindowFrameAutosave? = nil,
        configureWindow: (NSWindow) -> Void = { _ in },
        onClose: @escaping () -> Void
    ) {
        self.frameAutosave = frameAutosave
        self.onClose = onClose
        let hosting = NSHostingController(rootView: content)
        hosting.sizingOptions = sizing
        hosting.sceneBridgingOptions = bridging
        let window = NSWindow(contentViewController: hosting)
        configureWindow(window)
        if window.styleMask.contains(.fullSizeContentView) {
            // The view draws under the buttons and needs no title bar inset.
            hosting.safeAreaRegions.remove(.container)
        }
        window.setContentSize(
            contentSize
                ?? hosting.sizeThatFits(
                    in: CGSize(
                        width: CGFloat.infinity,
                        height: CGFloat.infinity
                    )
                )
        )
        // A window restored at launch would open without the primary one.
        window.isRestorable = false
        window.tabbingMode = .disallowed
        // Its owner lets it go once it closes.
        window.isReleasedWhenClosed = false
        super.init(window: window)
        if let frameAutosave {
            shouldCascadeWindows = false
            frameAutosave.place(window, contentSized: contentSize == nil)
        }
        else {
            window.center()
        }
        window.delegate = self
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    func windowWillClose(_ notice: Notification) {
        saveFrame(from: notice)
        onClose()
    }

    func windowDidMove(_ notice: Notification) {
        saveFrame(from: notice)
    }

    func windowDidEndLiveResize(_ notice: Notification) {
        saveFrame(from: notice)
    }

    private func saveFrame(from notice: Notification) {
        if let frameAutosave, let window = notice.object as? NSWindow {
            frameAutosave.save(window)
        }
    }
}

extension NSWindow {
    /// Lets the content reach the top of the window, under its buttons.
    func showOnlyWindowButtons() {
        styleMask.insert(.fullSizeContentView)
        titlebarAppearsTransparent = true
        titleVisibility = .hidden
    }
}
