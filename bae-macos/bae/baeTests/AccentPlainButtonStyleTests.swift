import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Accent plain button")
@MainActor
struct AccentPlainButtonStyleTests {
    /// A disabled plain button dims to bae's off opacity, as every other
    /// button of bae does, not to SwiftUI's own 0.5. The wanted pixel is the
    /// same label drawn at the off opacity by the same renderer.
    @Test("a disabled plain button dims to the off opacity")
    func disabledDimsToOff() throws {
        let dimmed = try pixel(of: plainButton().disabled(true))
        let wanted = try pixel(of: label().opacity(ThemeOpacity.off))
        #expect(
            abs(dimmed - wanted) < 0.01,
            "dimmed \(dimmed) wanted \(wanted)"
        )
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
        let popUpShown = try brightest(popUp(enabled: false))
        let popUpWanted = try brightest(
            popUp(enabled: true).opacity(ThemeOpacity.off)
        )
        let rowShown = try brightest(row(enabled: false))
        let rowWanted = try brightest(
            row(enabled: true).opacity(ThemeOpacity.off)
        )
        #expect(
            abs(popUpShown - popUpWanted) < 0.02,
            "pop-up \(popUpShown) wanted \(popUpWanted)"
        )
        #expect(
            abs(rowShown - rowWanted) < 0.02,
            "row \(rowShown) wanted \(rowWanted)"
        )
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

    private func label() -> some View {
        Rectangle().fill(Color.white).frame(width: 20, height: 20)
    }

    private func plainButton() -> some View {
        Button {
        } label: {
            label()
        }
        .buttonStyle(.accentPlain)
    }

    /// The red component at the middle of `content` on black, padded.
    private func pixel<Content: View>(of content: Content) throws -> Double {
        let renderer = ImageRenderer(
            content: content.padding(4).background(Color.black)
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
