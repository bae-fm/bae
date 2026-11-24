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

    /// A disabled pop-up button and a disabled row that opens something draw
    /// at the off opacity once, as the board does: their own labels do not
    /// dim again under the button style's dimming.
    @Test("a disabled pop-up and a disabled pressed row dim once")
    func dimOnce() throws {
        func popUp(enabled: Bool) -> some View {
            MacPopUpButton("Share with", selection: .constant("Only me")) {
                MacPopUpOption(verbatim: "Only me", value: "Only me")
            }
            .disabled(!enabled)
        }
        func row(enabled: Bool) -> some View {
            MacRow("Storage", value: "Cloud")
                .pressing(.opens {})
                .disabled(!enabled)
                .frame(width: 300)
        }
        let popUpRatio =
            try brightest(popUp(enabled: false))
            / brightest(popUp(enabled: true))
        let rowRatio =
            try brightest(row(enabled: false)) / brightest(row(enabled: true))
        #expect(abs(popUpRatio - ThemeOpacity.off) < 0.03)
        #expect(abs(rowRatio - ThemeOpacity.off) < 0.03)
    }

    /// The brightest colour component of `content` drawn on black.
    private func brightest<Content: View>(_ content: Content) throws -> Double {
        let renderer = ImageRenderer(
            content:
                content
                .appearance(mode: .dark, accent: .pink, tone: .midnight)
                .padding(4)
                .background(Color.black)
        )
        renderer.scale = 2
        let image = try #require(renderer.cgImage)
        let bitmap = NSBitmapImageRep(cgImage: image)
        var brightest = 0.0
        for x in 0..<image.width {
            for y in 0..<image.height {
                let colour = try #require(
                    bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB)
                )
                brightest = max(
                    brightest,
                    Double(
                        max(
                            colour.redComponent,
                            colour.greenComponent,
                            colour.blueComponent
                        )
                    )
                )
            }
        }
        return brightest
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
