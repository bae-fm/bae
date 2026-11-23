import AppKit
import BaeKit
import SwiftUI
import os.log

private let windowLogger = Logger.bae("HostingWindowController")

/// A window showing one SwiftUI view. Closing it tells its owner, which lets
/// it go, so the view and everything it started end with the window and
/// opening it again builds them anew.
@MainActor
class HostingWindowController: NSWindowController, NSWindowDelegate {
    struct FrameAutosave {
        let name: NSWindow.FrameAutosaveName
        let firstScreen: NSScreen?
    }

    private let onClose: () -> Void

    /// `content` in a titled window of `contentSize`, or of the content's own
    /// size where that is nil, sized within the limits `sizing` names, with
    /// the window parts `bridging` names managed by the view.
    init<Content: View>(
        content: Content,
        contentSize: CGSize?,
        sizing: NSHostingSizingOptions,
        bridging: NSHostingSceneBridgingOptions,
        frameAutosave: FrameAutosave? = nil,
        configureWindow: (NSWindow) -> Void = { _ in },
        onClose: @escaping () -> Void
    ) {
        self.onClose = onClose
        let hosting = NSHostingController(rootView: content)
        hosting.sizingOptions = sizing
        hosting.sceneBridgingOptions = bridging
        let window = NSWindow(contentViewController: hosting)
        configureWindow(window)
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
        window.delegate = self
        if let frameAutosave {
            shouldCascadeWindows = false
            Self.place(window, using: frameAutosave)
        }
        else {
            window.center()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    func windowWillClose(_: Notification) {
        onClose()
    }

    private static func place(_ window: NSWindow, using autosave: FrameAutosave)
    {
        // AppKit stores this value under the documented frame autosave key.
        // Read it before assigning the name, which also restores the frame.
        let saved = UserDefaults.standard.string(
            forKey: "NSWindow Frame \(autosave.name)"
        )
        if !window.setFrameAutosaveName(autosave.name) {
            windowLogger.error(
                "AppKit rejected the window frame autosave name \(autosave.name)"
            )
        }
        if let saved {
            restoreOnConnectedScreen(window, descriptor: saved)
        }
        else if let screen = autosave.firstScreen ?? NSScreen.screens.first {
            let room = screen.visibleFrame
            let size = CGSize(
                width: min(window.frame.width, room.width),
                height: min(window.frame.height, room.height)
            )
            window.setFrame(
                CGRect(
                    x: room.midX - size.width / 2,
                    y: room.midY - size.height / 2,
                    width: size.width,
                    height: size.height
                ),
                display: false
            )
        }
    }

    /// AppKit can move a saved window to the primary display even when its
    /// original display is still connected. Its descriptor stores the window
    /// frame followed by the visible screen frame. Use the exact frame when
    /// that screen is still present and the content still fits the saved size.
    /// Otherwise keep AppKit's restoration, which fits the current displays.
    private static func restoreOnConnectedScreen(
        _ window: NSWindow,
        descriptor: NSWindow.PersistableFrameDescriptor
    ) {
        let fields = descriptor.split(whereSeparator: \.isWhitespace)
        let coordinates = fields.compactMap { Double($0) }
        guard fields.count == 8, coordinates.count == fields.count,
            coordinates.allSatisfy(\.isFinite)
        else { return }
        let frame = CGRect(
            x: coordinates[0],
            y: coordinates[1],
            width: coordinates[2],
            height: coordinates[3]
        )
        let screenFrame = CGRect(
            x: coordinates[4],
            y: coordinates[5],
            width: coordinates[6],
            height: coordinates[7]
        )
        guard
            NSScreen.screens.contains(where: { $0.visibleFrame == screenFrame }
            ),
            frame.width >= window.minSize.width,
            frame.height >= window.minSize.height,
            frame.width <= window.maxSize.width,
            frame.height <= window.maxSize.height,
            frame != window.frame
        else { return }
        window.setFrame(frame, display: false)
    }
}
