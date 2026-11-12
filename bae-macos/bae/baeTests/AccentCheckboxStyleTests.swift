import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Accent checkbox")
@MainActor
struct AccentCheckboxStyleTests {
    /// A checked box is filled in the accent the window is drawn in, not the
    /// Mac's accent colour: the fill is the chosen accent's, whichever accent
    /// that is. The expected colour is the accent's fill drawn by the same
    /// renderer, so both sides go through one colour pipeline.
    @Test(
        "a checked box is filled in the chosen accent",
        arguments: [AccentChoice.coral, .mint]
    )
    func checkedBoxTakesTheAccent(accent: AccentChoice) throws {
        let box = try pixel(
            of: Toggle("Group by artist", isOn: .constant(true))
                .toggleStyle(.accentCheckbox),
            accent: accent,
            // Inside the box, left of the check: 4 points of padding and 2
            // into the box, at the middle of the row.
            x: 6
        )
        let wanted = try pixel(
            of: Rectangle().fill(accent.fillColor).frame(width: 14, height: 14),
            accent: accent,
            x: 6
        )
        #expect(abs(box.redComponent - wanted.redComponent) < 0.01)
        #expect(abs(box.greenComponent - wanted.greenComponent) < 0.01)
        #expect(abs(box.blueComponent - wanted.blueComponent) < 0.01)
    }

    /// The colour of the pixel `x` points in from the left edge, at the
    /// middle of the height, of `content` drawn dark in `accent` on 4 points
    /// of padding.
    private func pixel<Content: View>(
        of content: Content,
        accent: AccentChoice,
        x: Int
    ) throws -> NSColor {
        let renderer = ImageRenderer(
            content:
                content
                .appearance(mode: .dark, accent: accent, tone: .midnight)
                .padding(4)
        )
        renderer.scale = 2
        let image = try #require(renderer.cgImage)
        let bitmap = NSBitmapImageRep(cgImage: image)
        return try #require(
            bitmap.colorAt(x: 2 * x, y: image.height / 2)?
                .usingColorSpace(.sRGB)
        )
    }
}
