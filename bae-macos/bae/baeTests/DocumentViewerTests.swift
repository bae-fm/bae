import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct DocumentViewerTests {
    @Test("A log chip opens an opaque document panel")
    func chipDocumentPanel() async throws {
        let store = UiStore()
        let action = ReleaseEvidenceAction(
            read: { _, _ in
                [.document(name: "rip.log", text: "Extraction log")]
            },
            uiStore: store
        )
        action(.candidate(key: "candidate"), .verification)
        for _ in 0..<1000 {
            if store.modalBuilder != nil { break }
            await Task.yield()
        }
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

    @Test("File rows and log chips present the same document panel")
    func sameDocumentPresentation() async throws {
        let fileStore = UiStore()
        fileStore.presentDocument(name: "rip.log", text: "Extraction log")
        let chipStore = UiStore()
        let action = ReleaseEvidenceAction(
            read: { _, _ in
                [.document(name: "rip.log", text: "Extraction log")]
            },
            uiStore: chipStore
        )
        action(.release(id: "release"), .verification)
        for _ in 0..<1000 {
            if chipStore.modalBuilder != nil { break }
            await Task.yield()
        }
        let filePanel = try #require(fileStore.modalBuilder)
        let chipPanel = try #require(chipStore.modalBuilder)
        let fileImage = ImageRenderer(
            content: filePanel().preferredColorScheme(.dark)
        )
        let chipImage = ImageRenderer(
            content: chipPanel().preferredColorScheme(.dark)
        )
        let fileBitmap = NSBitmapImageRep(
            cgImage: try #require(fileImage.cgImage)
        )
        let chipBitmap = NSBitmapImageRep(
            cgImage: try #require(chipImage.cgImage)
        )
        #expect(
            try #require(
                fileBitmap.representation(using: .png, properties: [:])
            )
                == #require(
                    chipBitmap.representation(using: .png, properties: [:])
                )
        )
    }

    @Test("Scan chips use the file image lightbox")
    func scanLightbox() async throws {
        let store = UiStore()
        let action = ReleaseEvidenceAction(
            read: { _, _ in [.image(name: "back.png", bytes: Data([1, 2, 3]))]
            },
            uiStore: store
        )
        action(.candidate(key: "candidate"), .verification)
        for _ in 0..<1000 {
            if store.lightbox != nil { break }
            await Task.yield()
        }
        let item = try #require(store.lightbox?.current)
        #expect(item.label == "back.png")
        #expect(item.previewContent == .bytes(Data([1, 2, 3])))
        #expect(store.modalBuilder == nil)
    }
}
