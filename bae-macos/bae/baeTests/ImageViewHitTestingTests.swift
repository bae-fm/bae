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
        try await SnapshotTestSupport.withHostedWindow(
            Button(action: { tapBox.tapped = true }) {
                ImageView(imageRef: nil, pointSize: side)
                    .frame(width: side, height: side)
            }
            .buttonStyle(.plain)
            .environment(ImageStore.stub())
            .environment(\.colorScheme, .dark),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)

            let bounds = NSRect(origin: .zero, size: size)
            let center = NSPoint(x: bounds.midX, y: bounds.midY)

            try HostedInput.click(at: center, in: window)
            try await Wait.until { tapBox.tapped }
        }
    }
}

/// A cover whose load failed — an archive answering 503 — offers another try
/// on the slot itself: clicking the failed placeholder asks the store again,
/// and the art the second answer carries is what the slot then shows.
@Suite("ImageView retry")
struct ImageViewRetryTests {
    @MainActor
    @Test("clicking a failed cover asks for it again")
    func clickingAFailedCoverAsksAgain() async throws {
        let side: CGFloat = 120
        let size = NSSize(width: side, height: side)
        let attempts = AttemptCount()
        let bytes = try solidPNG()
        let store = ImageStore(fetchRemoteImage: { _, _ in
            let attempt = await attempts.next()
            if attempt == 1 {
                throw URLError(.badServerResponse)
            }
            return bytes
        })
        let content = ImageContent.remote(
            BridgeRemoteImageSet(
                url: "https://images.example/front.jpg",
                downscaled: []
            )
        )
        try await SnapshotTestSupport.withHostedWindow(
            ImageView(content: content, pointSize: side)
                .frame(width: side, height: side)
                .environment(store),
            size: size
        ) { window, host in
            try await Wait.until { await attempts.count == 1 }
            try await SnapshotTestSupport.settle(host)

            try HostedInput.click(
                at: NSPoint(x: side / 2, y: side / 2),
                in: window
            )
            try await Wait.until { await attempts.count == 2 }
            try await Wait.until {
                store.cachedImage(content, pointSize: side, displayScale: 2)
                    != nil
                    || store.cachedImage(
                        content,
                        pointSize: side,
                        displayScale: 1
                    ) != nil
            }
        }
    }

    private func solidPNG() throws -> Data {
        let bitmap = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: 8,
                pixelsHigh: 8,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            )
        )
        return try #require(bitmap.representation(using: .png, properties: [:]))
    }
}

private actor AttemptCount {
    private(set) var count = 0

    func next() -> Int {
        count += 1
        return count
    }
}

@MainActor
private final class TapBox {
    var tapped = false
}
