import AppKit
import BaeKit
import SwiftUI

/// Moves the window's traffic-light buttons (close, minimize, zoom) by an
/// offset. With `.hiddenTitleBar` the buttons sit at the system's place, which
/// doesn't line up with our custom title bar content.
struct TrafficLightOffset: ViewModifier {
    let xOffset: CGFloat
    let yOffset: CGFloat

    func body(content: Content) -> some View {
        content
            .background(
                TrafficLightPositioner(xOffset: xOffset, yOffset: yOffset)
            )
    }
}

private struct TrafficLightPositioner: NSViewRepresentable {
    let xOffset: CGFloat
    let yOffset: CGFloat

    func makeNSView(context _: Context) -> PositionerView {
        PositionerView(xOffset: xOffset, yOffset: yOffset)
    }

    func updateNSView(_: PositionerView, context _: Context) {}

    /// Moves the buttons of the window it is in when it joins that window,
    /// and again after each resize of it, since AppKit puts them back at
    /// their usual place when it resizes the window.
    final class PositionerView: NSView {
        private let xOffset: CGFloat
        private let yOffset: CGFloat
        private var resizeObservation: (any NSObjectProtocol)?

        init(xOffset: CGFloat, yOffset: CGFloat) {
            self.xOffset = xOffset
            self.yOffset = yOffset
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not supported")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopObservingResize()
            guard let window else {
                return
            }
            moveButtons(in: window)
            resizeObservation = NotificationCenter.default.addObserver(
                forName: NSWindow.didResizeNotification,
                object: window,
                queue: .main
            ) { [weak self] notification in
                guard let window = notification.object as? NSWindow else {
                    return
                }
                MainActor.assumeIsolated {
                    self?.moveButtons(in: window)
                }
            }
        }

        /// A window that closes lets go of its views without moving them out
        /// of it first.
        isolated deinit {
            stopObservingResize()
        }

        private func stopObservingResize() {
            if let resizeObservation {
                NotificationCenter.default.removeObserver(resizeObservation)
                self.resizeObservation = nil
            }
        }

        private func moveButtons(in window: NSWindow) {
            for buttonType: NSWindow.ButtonType in [
                .closeButton, .miniaturizeButton, .zoomButton,
            ] {
                guard let button = window.standardWindowButton(buttonType)
                else {
                    continue
                }
                var origin = button.frame.origin
                origin.x += xOffset
                origin.y -= yOffset
                button.setFrameOrigin(origin)
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// The modifier repositions the real window's traffic-light buttons via
    /// AppKit; the preview canvas has no `NSWindow`, so the positioner never
    /// joins one here and this hosts a sample surface only to confirm the
    /// modifier composes and leaves its content unchanged.
    #Preview("Hosted") {
        Rectangle()
            .fill(Theme.surface)
            .frame(width: 400, height: 120)
            .modifier(TrafficLightOffset(xOffset: 12, yOffset: 8))
    }
#endif
