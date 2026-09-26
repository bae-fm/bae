import AppKit
import SwiftUI
import Testing
import Vision

/// Shared AppKit hosting + snapshot helpers for the view tests.
enum SnapshotTestSupport {
    /// `host`'s pixels once they hold still, as PNG bytes over its window's
    /// background: the way a person sees it.
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
        size: NSSize
    ) async throws -> Data {
        let view = try await steadyBitmap(of: host, size: size)
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

    /// Settle `host`, then capture it until its pixels read the same for
    /// `Wait.steadiness`.
    ///
    /// Settled frames are not settled pixels. Content a view loads on its own
    /// — a cover, a placeholder once a load comes back empty — draws inside a
    /// frame that never moves, so the capture is retaken until it holds.
    @MainActor
    static func steadyBitmap(
        of host: NSView,
        size: NSSize,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws -> NSBitmapImageRep {
        try await settle(host, file: file, line: line)
        var capture = try bitmap(of: host, size: size)
        try await Wait.untilSteady(file: file, line: line) {
            capture = try bitmap(of: host, size: size)
            return try pixelBytes(of: capture)
        }
        return capture
    }

    /// A capture's raw pixels, for telling two captures apart.
    private static func pixelBytes(of bitmap: NSBitmapImageRep) throws -> Data {
        let bytes = try #require(bitmap.bitmapData)
        return Data(bytes: bytes, count: bitmap.bytesPerPlane)
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
        try autoreleasepool {
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
    }

    /// Have every layer below `layer` that draws itself draw at
    /// `captureScale`. A layer already at that scale is left alone, so a
    /// display that draws at it captures exactly what it shows; one that is
    /// not redraws now, so the capture that follows copies contents drawn at
    /// the new scale rather than whatever the next transaction would have
    /// replaced. A layer handed an image keeps it: asking it to display
    /// again replaces the image with an empty backing store, and a glyph
    /// the row had drawn was gone from the capture.
    static func rescale(_ layer: CALayer) {
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
    /// interacts with it: lay the tree out until no frame in it has moved
    /// for `Wait.steadiness`.
    ///
    /// SwiftUI lays a hosted tree out over several main-actor turns — a
    /// geometry reader publishes a width, the views under it re-measure, a
    /// text wraps — and work a view starts on a task lands a few turns later
    /// and moves a row by a line. How many turns that takes depends on the
    /// machine, so convergence is what is waited for. A tree that never
    /// converges fails the wait rather than being inspected mid-layout.
    @MainActor
    static func settle(
        _ host: NSView,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        try await Wait.untilSteady(file: file, line: line) {
            autoreleasepool {
                host.layoutSubtreeIfNeeded()
                return descendants(of: host).map(\.frame)
            }
        }
    }

    /// Every AppKit view below `view`, depth first. SwiftUI controls may be
    /// nested below private hosting containers, so interaction tests use the
    /// full hosted tree rather than assuming one framework-specific depth.
    @MainActor
    static func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }

    /// Have a SwiftUI-backed pop-up menu build its current items, without
    /// showing it.
    ///
    /// SwiftUI fills the menu in the pop-up cell's delegate call that comes
    /// just before the menu is shown, and in no earlier hook: neither the
    /// menu delegate's update and open calls nor the will-pop-up
    /// notification add an item. Opening the menu for real puts it on the
    /// screen of the person running the suite, pulled in from wherever the
    /// hosting window sits, so the helper makes that one call itself. A
    /// system that stops making it leaves the menu empty, and the test
    /// reading it fails on the missing items.
    @MainActor
    static func populateMenu(_ button: NSPopUpButton) {
        guard let menu = button.menu,
            let cell = button.cell as? NSPopUpButtonCell,
            let delegate = cell.value(forKey: "delegate") as? NSObject
        else { return }
        _ = delegate.perform(
            NSSelectorFromString("popUpButtonCell:willShowMenu:"),
            with: cell,
            with: menu
        )
    }

    /// One line of text Vision read off a capture: the words, and where on
    /// the image they sit (Vision's normalized box, origin at the bottom left).
    struct RecognizedLine: Sendable {
        let text: String
        let boundingBox: CGRect
    }

    /// The lines of text drawn in `png`, read by Vision's accurate recognizer
    /// on a dispatch queue, off the main actor the capture was made on.
    static func recognizedText(
        in png: Data,
        languages: [String]? = nil
    ) async throws -> [RecognizedLine] {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated)
                .async {
                    continuation.resume(
                        with: Result {
                            try recognize(png, languages: languages)
                        }
                    )
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
