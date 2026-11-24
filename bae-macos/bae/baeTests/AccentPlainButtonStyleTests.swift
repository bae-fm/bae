import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Accent plain button")
@MainActor
struct AccentPlainButtonStyleTests {
    /// A disabled plain button dims to bae's off opacity, as every other
    /// button of bae does, not to SwiftUI's own 0.5. The label is white on
    /// black, so the pixel's red component is the opacity.
    @Test("a disabled plain button dims to the off opacity")
    func disabledDimsToOff() throws {
        let dimmed = try pixel(enabled: false)
        let whole = try pixel(enabled: true)
        #expect(abs(whole - 1) < 0.02)
        #expect(abs(dimmed - ThemeOpacity.off) < 0.02)
    }

    private func pixel(enabled: Bool) throws -> Double {
        let renderer = ImageRenderer(
            content: Button {
            } label: {
                Rectangle().fill(Color.white).frame(width: 20, height: 20)
            }
            .buttonStyle(.accentPlain)
            .disabled(!enabled)
            .padding(4)
            .background(Color.black)
        )
        renderer.scale = 2
        let image = try #require(renderer.cgImage)
        let bitmap = NSBitmapImageRep(cgImage: image)
        let colour = try #require(
            bitmap.colorAt(x: image.width / 2, y: image.height / 2)?
                .usingColorSpace(.sRGB)
        )
        return Double(colour.redComponent)
    }
}
