import AppKit
import SwiftUI
import Testing
import Vision

/// Shared AppKit hosting + snapshot helpers for the view tests.
enum SnapshotTestSupport {
    /// Host `view` (sized to `size`) in a borderless key window. The caller keeps
    /// the returned window alive for the test's duration and uses the host to
    /// capture pixels or send events through the window.
    ///
    /// The tree's layers are set to draw at `captureScale` as soon as they
    /// exist, so the redraw that a display at another scale needs happens
    /// while the caller settles the view, not between two captures a test
    /// compares pixel for pixel.
    @MainActor
    static func hostInWindow<V: View>(
        _ view: V,
        size: NSSize
    ) -> (window: NSWindow, host: NSHostingView<V>) {
        let bounds = NSRect(origin: .zero, size: size)
        let host = NSHostingView(rootView: view)
        host.frame = bounds
        let window = SnapshotTestWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        window.makeKeyAndOrderFront(nil)
        host.layoutSubtreeIfNeeded()
        if let layer = host.layer {
            rescale(layer)
        }
        return (window, host)
    }

    /// Lay out `host` and capture it as PNG bytes for text recognition: the
    /// view over its window's background, the way a person sees it. Yields
    /// once so SwiftUI's async work settles, and sleeps `waitNanoseconds`
    /// first when the view has async content (a cover load) that must
    /// resolve before the capture.
    ///
    /// The window's background is painted under the view because the view
    /// alone is transparent where it draws nothing, and text in a label
    /// colour over transparency reads by appearance: the recognizer sees the
    /// light text of the dark appearance and misses the dark text of the
    /// light one, so a capture that read on a dark development machine read
    /// as empty on a light hosted runner. Over the window's own colour the
    /// words have the same contrast in either appearance.
    ///
    /// Composed in a Core Graphics context addressed in pixels: a fill
    /// through the bitmap's AppKit graphics context painted nothing on the
    /// hosted runner and left the capture transparent, while the same fill
    /// painted on a development machine.
    @MainActor
    static func capturePNG(
        _ host: NSView,
        size: NSSize,
        waitNanoseconds: UInt64 = 0
    ) async throws -> Data {
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        if waitNanoseconds > 0 {
            try await Task.sleep(nanoseconds: waitNanoseconds)
        }
        host.layoutSubtreeIfNeeded()
        let view = try bitmap(of: host, size: size)
        let viewImage = try #require(view.cgImage)
        let pixels = CGRect(
            x: 0,
            y: 0,
            width: view.pixelsWide,
            height: view.pixelsHigh
        )
        let space = try #require(view.colorSpace.cgColorSpace)
        let context = try #require(
            CGContext(
                data: nil,
                width: view.pixelsWide,
                height: view.pixelsHigh,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
        )
        let backdrop = try windowBackground(for: host, in: view.colorSpace)
        context.setFillColor(backdrop.cgColor)
        context.fill(pixels)
        context.draw(viewImage, in: pixels)
        let composedImage = try #require(context.makeImage())
        let composed = NSBitmapImageRep(cgImage: composedImage)
        composed.size = size
        return try #require(
            composed.representation(using: .png, properties: [:])
        )
    }

    /// Pixels per point in every capture. Fixed rather than read from the
    /// window's screen so every machine draws the same pixels: a Retina
    /// development machine gives 2x, a hosted runner's display 1x, and at 1x
    /// a caption-sized word is ten pixels tall, which the text recognizer
    /// reads as other letters.
    static let captureScale = 2

    /// The pixels `host` paints, at `captureScale`, in the colour space its
    /// display would draw them in. Transparent where the view draws nothing.
    ///
    /// The colour space is the display's rather than sRGB because converting
    /// a capture to sRGB changed what the recognizer read: a thin grey
    /// catalog number read correctly in the display's space and with one
    /// digit wrong after conversion, over any backdrop or none.
    ///
    /// The hosted tree's layers draw their contents at the display's scale
    /// and a capture only copies those contents, so on a 1x display a 2x
    /// bitmap held text rasterized at 1x and stretched, which the recognizer
    /// misread by a glyph. Every layer is told to draw at `captureScale`
    /// first, so the capture carries text rendered at that scale wherever
    /// the window sits.
    @MainActor
    static func bitmap(of host: NSView, size: NSSize) throws -> NSBitmapImageRep
    {
        let bounds = NSRect(origin: .zero, size: size)
        let layer = try #require(host.layer)
        rescale(layer)
        host.displayIfNeeded()
        let matched = try #require(
            host.bitmapImageRepForCachingDisplay(in: bounds)
        )
        let bitmap = try bitmap(size: size, in: matched.colorSpace)
        host.cacheDisplay(in: bounds, to: bitmap)
        return bitmap
    }

    /// Have every layer below `layer` that draws itself draw at
    /// `captureScale`. A layer already at that scale is left alone, so a
    /// display that draws at it captures exactly what it shows; one that is
    /// not redraws now, so the capture that follows copies contents drawn at
    /// the new scale rather than whatever the next transaction would have
    /// replaced. A layer handed an image keeps it: asking it to display
    /// again replaces the image with an empty backing store, and a glyph
    /// the row had drawn was gone from the capture.
    private static func rescale(_ layer: CALayer) {
        if !holdsImage(layer), layer.contentsScale != CGFloat(captureScale) {
            layer.contentsScale = CGFloat(captureScale)
            layer.setNeedsDisplay()
            layer.displayIfNeeded()
        }
        layer.sublayers?.forEach(rescale)
    }

    /// Whether `layer` shows an image it was handed rather than one it drew:
    /// its contents are a `CGImage`, where a layer that draws itself holds
    /// the backing store its drawing filled.
    private static func holdsImage(_ layer: CALayer) -> Bool {
        guard let contents = layer.contents else { return false }
        return CFGetTypeID(contents as CFTypeRef) == CGImage.typeID
    }

    /// The window background colour under `host`'s appearance, as concrete
    /// components in `space`: a dynamic colour resolves against the
    /// appearance current when it is drawn, and a bitmap context has none of
    /// its own.
    @MainActor
    private static func windowBackground(
        for host: NSView,
        in space: NSColorSpace
    ) throws -> NSColor {
        var resolved: NSColor?
        host.effectiveAppearance.performAsCurrentDrawingAppearance {
            resolved = NSColor.windowBackgroundColor.usingColorSpace(space)
        }
        let backdrop = try #require(resolved)
        try #require(backdrop.alphaComponent == 1)
        return backdrop
    }

    /// An empty bitmap in `space`, `captureScale` pixels per point of `size`.
    private static func bitmap(size: NSSize, in space: NSColorSpace) throws
        -> NSBitmapImageRep
    {
        let bitmap = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: Int(size.width) * captureScale,
                pixelsHigh: Int(size.height) * captureScale,
                bitsPerSample: 8,
                samplesPerPixel: 4,
                hasAlpha: true,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            )?
            .retagging(with: space)
        )
        bitmap.size = size
        return bitmap
    }

    /// Let SwiftUI publish its renders before a hosted-view test inspects or
    /// interacts with it: yield until a turn changes no frame in the hosted
    /// tree.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and how many turns that takes depends on the machine. A
    /// fixed number of yields measured a tree mid-layout on a slow runner and
    /// gave frames a few points off. Convergence is what "settled" means, so
    /// that is what is waited for, after the floor of turns every hosted
    /// view needs to publish at all. A view that animates never converges;
    /// `maxTurns` bounds the wait and leaves it as the last turn drew it.
    @MainActor
    /// Lay the tree out until nothing in it has moved for `quietTurns`
    /// consecutive turns, each a run-loop yield plus a few milliseconds.
    /// One unchanged turn is not enough: work a view kicks off on a task
    /// lands a few turns later and moves a row by a line, and a capture
    /// taken before it reads a layout nobody will ever see.
    static func settle(
        _ host: NSView,
        minimumTurns: Int = 3,
        quietTurns: Int = 5,
        maxTurns: Int = 240
    ) async {
        var previous: [CGRect]?
        var quiet = 0
        for turn in 0..<maxTurns {
            host.layoutSubtreeIfNeeded()
            let frames = descendants(of: host).map(\.frame)
            quiet = frames == previous ? quiet + 1 : 0
            if turn >= minimumTurns, quiet >= quietTurns {
                return
            }
            previous = frames
            await Task.yield()
            try? await Task.sleep(for: .milliseconds(8))
        }
        host.layoutSubtreeIfNeeded()
    }

    /// Every AppKit view below `view`, depth first. SwiftUI controls may be
    /// nested below private hosting containers, so interaction tests use the
    /// full hosted tree rather than assuming one framework-specific depth.
    @MainActor
    static func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }

    /// Open and dismiss a SwiftUI-backed menu so its current items are available.
    @MainActor
    static func populateMenu(_ button: NSPopUpButton) {
        let cancel = Timer(timeInterval: 0.1, repeats: false) { _ in
            MainActor.assumeIsolated { button.menu?.cancelTracking() }
        }
        RunLoop.main.add(cancel, forMode: .common)
        button.performClick(nil)
        cancel.invalidate()
    }

    /// One line of text Vision read off a capture: the words, and where on
    /// the image they sit (Vision's normalized box, origin at the bottom left).
    /// A value rather than the observation itself, so it can cross the task
    /// that races the recognizer against its deadline.
    struct RecognizedLine: Sendable {
        let text: String
        let boundingBox: CGRect
    }

    /// Vision's recognizer did not answer within the deadline.
    ///
    /// The recognizer runs in-process on a system service, and that service
    /// wedges now and then on this machine: the request parks on a semaphore
    /// that is never signalled and the whole test host sits there until
    /// somebody notices, hours later. A deadline turns that into one failed
    /// test with this error as its reason.
    struct TextRecognitionTimedOut: Error, CustomStringConvertible {
        let after: TimeInterval

        var description: String {
            "Vision text recognition did not answer within \(after)s; "
                + "the recognizer service is wedged, not the view under test"
        }
    }

    /// The lines of text drawn in `png`, read by Vision's accurate recognizer.
    ///
    /// The recognition runs on a dispatch queue and is raced against
    /// `timeout`: whichever finishes first resumes the caller, and the other
    /// finds the continuation already claimed. Dispatch rather than a task
    /// group: a group waits for every child before it returns, and a child
    /// parked inside a wedged recognizer never returns, so a deadline thrown
    /// from a sibling could not end it. A request that has wedged cannot be
    /// unblocked at all; its thread is abandoned, and the test host is a
    /// throwaway process.
    static func recognizedText(
        in png: Data,
        languages: [String]? = nil,
        timeout: TimeInterval = 60
    ) async throws -> [RecognizedLine] {
        let first = FirstToFinish()
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated)
                .async {
                    let outcome = Result {
                        try recognize(png, languages: languages)
                    }
                    if first.claim() {
                        continuation.resume(with: outcome)
                    }
                }
            DispatchQueue.global()
                .asyncAfter(deadline: .now() + timeout) {
                    if first.claim() {
                        continuation.resume(
                            throwing: TextRecognitionTimedOut(after: timeout)
                        )
                    }
                }
        }
    }

    private static func recognize(
        _ png: Data,
        languages: [String]?
    ) throws -> [RecognizedLine] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        if let languages {
            request.recognitionLanguages = languages
        }
        try VNImageRequestHandler(data: png, options: [:]).perform([request])
        return (request.results ?? [])
            .map { observation in
                RecognizedLine(
                    text: observation.topCandidates(1).first?.string ?? "",
                    boundingBox: observation.boundingBox
                )
            }
    }

    /// Which of two racing tasks gets to resume a continuation: the first to
    /// claim, exactly once.
    private final class FirstToFinish: @unchecked Sendable {
        private let lock = NSLock()
        private var claimed = false

        func claim() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            if claimed {
                return false
            }
            claimed = true
            return true
        }
    }
}

extension Collection<String> {
    /// Whether any of these lines carries `text`.
    ///
    /// Text recognition reads a line as it was drawn, glyphs included: a
    /// section header's symbol, an error's warning triangle, a checkbox's box
    /// — each shares a baseline with its words and comes back glued to them,
    /// and how much of the glyph survives depends on the machine that drew
    /// the pixels. A check that the words were drawn asks whether a line
    /// carries them, not whether one equals them.
    func carrying(_ text: String) -> Bool {
        contains { $0.contains(text) }
    }
}

private final class SnapshotTestWindow: NSWindow {
    override var canBecomeKey: Bool { true }
}
