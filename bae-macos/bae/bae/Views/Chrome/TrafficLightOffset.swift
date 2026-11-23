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
    /// and again whenever AppKit has put them back at their usual place:
    /// AppKit lays the title bar out again as the window resizes, becomes key
    /// or updates, and each layout puts every button back.
    final class PositionerView: NSView {
        private let xOffset: CGFloat
        private let yOffset: CGFloat
        /// Where this view last put each button. A button at another place
        /// is at its usual place, which AppKit put it back to.
        private var placed: [NSWindow.ButtonType: CGPoint] = [:]
        private var noticeWatch: WindowNoticeWatch?

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
            noticeWatch = nil
            placed = [:]
            guard let window else {
                return
            }
            moveButtons(in: window)
            noticeWatch = WindowNoticeWatch(
                of: window,
                notices: [
                    NSWindow.didResizeNotification,
                    NSWindow.didBecomeKeyNotification,
                    NSWindow.didUpdateNotification,
                ]
            ) { [weak self] window in
                self?.moveButtons(in: window)
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
                guard button.frame.origin != placed[buttonType] else {
                    continue
                }
                var origin = button.frame.origin
                origin.x += xOffset
                origin.y -= yOffset
                button.setFrameOrigin(origin)
                placed[buttonType] = origin
            }
        }
    }
}
