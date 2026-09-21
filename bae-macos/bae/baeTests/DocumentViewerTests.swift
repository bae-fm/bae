import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct DocumentViewerTests {
    @Test("A file row opens an opaque document panel")
    func fileRowDocumentPanel() async throws {
        let store = UiStore()
        store.presentDocument(name: "rip.log", text: "Extraction log")
        let builder = try #require(store.modalBuilder)
        let renderer = ImageRenderer(
            content: builder()
                .preferredColorScheme(.dark)
                .background(Color.green)
        )
        let bitmap = NSBitmapImageRep(cgImage: try #require(renderer.cgImage))
        let pixel = try #require(
            bitmap.colorAt(x: bitmap.pixelsWide / 2, y: bitmap.pixelsHigh / 2)?
                .usingColorSpace(.deviceRGB)
        )
        #expect(abs(pixel.greenComponent - pixel.redComponent) < 0.05)
        #expect(abs(pixel.blueComponent - pixel.redComponent) < 0.05)
        #expect(pixel.alphaComponent == 1)
        #expect(bitmap.pixelsWide == 750)
        #expect(bitmap.pixelsHigh == 600)
        store.dismissModal()
        #expect(store.modalBuilder == nil)
    }
}
