import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct DocumentViewerTests {
    /// The panel is the window's size; that it is opaque over what it
    /// covers is the `document-viewer` shot's.
    @Test("A file row opens a document panel, and closing it dismisses it")
    func fileRowDocumentPanel() throws {
        let store = UiStore()
        store.presentDocument(name: "rip.log", text: "Extraction log")
        let builder = try #require(store.modalBuilder)
        let panel = NSHostingView(rootView: builder())
        #expect(panel.fittingSize == NSSize(width: 750, height: 600))
        store.dismissModal()
        #expect(store.modalBuilder == nil)
    }
}
