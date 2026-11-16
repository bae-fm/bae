import AppKit
import SwiftUI
import os.log

private let logger = Logger.bae("SettingsWindowButtonsPlacer")

extension View {
    /// Moves the window's close, minimise and zoom buttons to where the boards
    /// draw them: inset from the sidebar panel's top left corner, in the room
    /// the sidebar leaves at its top. The window's title bar is transparent,
    /// which puts the buttons where a title bar's buttons sit, tight against
    /// the window's corner and over the panel's edge.
    func settingsWindowButtonsInSidebar() -> some View {
        background(SettingsWindowButtonsPlacer())
    }
}

/// An empty view that moves the buttons of the window it is in.
private struct SettingsWindowButtonsPlacer: NSViewRepresentable {
    func makeNSView(context _: Context) -> PlacerView {
        PlacerView()
    }

    func updateNSView(_: PlacerView, context _: Context) {}

    final class PlacerView: NSView {
        /// How far the first button's left edge and top edge sit from the
        /// window's: the sidebar's 8 pt margin and the boards' 12 pt inset
        /// from the panel's corner.
        private static let inset = CGPoint(x: 20, y: 20)

        private var updates: NSObjectProtocol?
        private var reportedMissing = false

        private static func name(of kind: NSWindow.ButtonType) -> String {
            switch kind {
            case .closeButton: "close"
            case .miniaturizeButton: "minimise"
            case .zoomButton: "zoom"
            default: "button \(kind.rawValue)"
            }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let updates {
                NotificationCenter.default.removeObserver(updates)
                self.updates = nil
            }
            guard let window else { return }
            place(in: window)
            // The window lays its title bar out again when it resizes, becomes
            // key or leaves full screen, and puts the buttons back; after each
            // of those the window posts an update, and this view moves the
            // buttons again.
            updates = NotificationCenter.default.addObserver(
                forName: NSWindow.didUpdateNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, let window = self.window else { return }
                    self.place(in: window)
                }
            }
        }

        /// Moves all three buttons by what moves the first to `inset`, so the
        /// space the system leaves between them stays. When the buttons are
        /// already there, the function does not move them.
        private func place(in window: NSWindow) {
            let kinds: [NSWindow.ButtonType] = [
                .closeButton, .miniaturizeButton, .zoomButton,
            ]
            let found = kinds.map { ($0, window.standardWindowButton($0)) }
            let buttons = found.compactMap(\.1)
            guard buttons.count == kinds.count, let first = buttons.first,
                let bar = first.superview
            else {
                // The window updates many times a second; one warning says
                // it.
                if !reportedMissing {
                    reportedMissing = true
                    let missing = found.filter { $0.1 == nil }
                        .map { Self.name(of: $0.0) }
                    logger.warning(
                        """
                        Settings window \(window.title) leaves its buttons \
                        where they are: missing \
                        \(missing.joined(separator: ", ")), or the buttons \
                        are in no view
                        """
                    )
                }
                return
            }
            let frame = first.convert(first.bounds, to: nil)
            let moveRight = Self.inset.x - frame.minX
            let moveUp = (window.frame.height - Self.inset.y) - frame.maxY
            guard abs(moveRight) >= 0.5 || abs(moveUp) >= 0.5 else { return }
            let sign: CGFloat = bar.isFlipped ? -1 : 1
            for button in buttons {
                button.setFrameOrigin(
                    NSPoint(
                        x: button.frame.origin.x + moveRight,
                        y: button.frame.origin.y + moveUp * sign
                    )
                )
            }
        }
    }
}
