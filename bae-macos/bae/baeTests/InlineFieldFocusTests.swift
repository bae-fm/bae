import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An inline field's words stay where they are when the person clicks into
/// the field and leaves it again: the field editor that AppKit places while
/// the field is edited draws the first glyph where the cell drew it at rest.
@MainActor
@Suite("Inline field focus")
struct InlineFieldFocusTests {
    private static let size = NSSize(width: 300, height: 60)
    private static let leading: CGFloat = 20

    /// The box of the dark ink in the first letters of the field, in points
    /// from the hosted view's top left. Reads the columns from 3 points
    /// before the words' column to 20 points after, which holds no ring and,
    /// with the caret at the end of the text, no caret.
    private static func firstGlyphInk(of host: NSView) throws -> CGRect {
        let rep = try #require(
            host.bitmapImageRepForCachingDisplay(in: host.bounds)
        )
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        var box = CGRect.null
        let from = Int((leading - 3) * scale)
        let to = Int((leading + 20) * scale)
        for y in 0..<rep.pixelsHigh {
            for x in from..<to {
                guard
                    let color = rep.colorAt(x: x, y: y)?
                        .usingColorSpace(.sRGB)
                else { continue }
                let luminance =
                    0.299 * color.redComponent + 0.587 * color.greenComponent
                    + 0.114 * color.blueComponent
                if luminance < 0.45 {
                    box = box.union(
                        CGRect(
                            x: CGFloat(x) / scale,
                            y: CGFloat(y) / scale,
                            width: 1 / scale,
                            height: 1 / scale
                        )
                    )
                }
            }
        }
        return box
    }

    @Test("the first glyph does not move when focus enters or leaves")
    func theFirstGlyphDoesNotMove() async throws {
        let value = "Harbor Records"
        try await SnapshotTestSupport.withHostedWindow(
            MacInlineField(
                placeholder: "—",
                value: value,
                fitsValue: true,
                onCommit: { _ in }
            )
            .padding(.leading, Self.leading)
            .padding(.top, 15)
            .frame(
                width: Self.size.width,
                height: Self.size.height,
                alignment: .topLeading
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: Self.size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first { $0.stringValue == value }
            )
            let atRest = try Self.firstGlyphInk(of: host)
            #expect(!atRest.isNull)

            #expect(window.makeFirstResponder(field))
            let editor = try #require(field.currentEditor())
            #expect(editor.superview != nil)
            editor.selectedRange = NSRange(location: value.count, length: 0)
            try await SnapshotTestSupport.settle(host)
            let focused = try Self.firstGlyphInk(of: host)

            #expect(abs(focused.minX - atRest.minX) < 0.5)
            #expect(abs(focused.minY - atRest.minY) < 0.5)
            #expect(abs(focused.maxY - atRest.maxY) < 0.5)

            window.makeFirstResponder(nil)
            try await SnapshotTestSupport.settle(host)
            let after = try Self.firstGlyphInk(of: host)
            #expect(abs(after.minX - atRest.minX) < 0.5)
            #expect(abs(after.minY - atRest.minY) < 0.5)
        }
    }
}
