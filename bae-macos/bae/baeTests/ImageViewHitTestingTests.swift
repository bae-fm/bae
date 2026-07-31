import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A cover picker wraps `ImageView` in a `Button` and derives the whole tap
/// region from the image (see `CoverSheetView.remoteCoverOption`); the album
/// grid card does the same with `.onTapGesture`. If `ImageView` renders no
/// hittable pixels, a click at the cover's center falls through and the button
/// never fires. This drives a synthesized click into the real `ImageView`
/// inside that wrapper and asserts the button's action ran.
///
/// `NSHostingView.hitTest` is not load-bearing here: SwiftUI renders the whole
/// Button + ImageView tree into the single hosting view with no descendant
/// `NSView`, so it returns the hosting view itself whether or not the content
/// is hittable. Sending a real mouse event through the window exercises
/// SwiftUI's own hit-testing, which is where `.contentShape` takes effect.
@Suite("ImageView hit testing")
struct ImageViewHitTestingTests {
    @MainActor
    @Test("a click at the cover center fires the wrapping button")
    func coverCenterClickFiresButton() async throws {
        let side: CGFloat = 120
        let tapBox = TapBox()
        let size = NSSize(width: side, height: side)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            Button(action: { tapBox.tapped = true }) {
                ImageView(imageRef: nil, pointSize: side)
                    .frame(width: side, height: side)
            }
            .buttonStyle(.plain)
            .environment(ImageStore.stub)
            .environment(\.colorScheme, .dark),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let bounds = NSRect(origin: .zero, size: size)
        let center = NSPoint(x: bounds.midX, y: bounds.midY)

        func click(at point: NSPoint) {
            for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                let event = NSEvent.mouseEvent(
                    with: type,
                    location: point,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )!
                window.sendEvent(event)
            }
        }
        click(at: center)
        try await Task.sleep(nanoseconds: 50_000_000)
        await Task.yield()
        #expect(tapBox.tapped)
    }
}

@MainActor
private final class TapBox {
    var tapped = false
}
