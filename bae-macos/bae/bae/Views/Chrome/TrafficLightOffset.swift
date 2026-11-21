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
        private var resizeWatch: WindowResizeWatch?

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
            resizeWatch = nil
            guard let window else {
                return
            }
            moveButtons(in: window)
            resizeWatch = WindowResizeWatch(of: window) { [weak self] window in
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
                var origin = button.frame.origin
                origin.x += xOffset
                origin.y -= yOffset
                button.setFrameOrigin(origin)
            }
        }
    }
}
