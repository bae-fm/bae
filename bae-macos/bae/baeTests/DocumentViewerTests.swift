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
        let document = try #require(store.document)
        let panel = NSHostingView(
            rootView: DocumentViewerView(
                name: document.name,
                text: document.text,
                onClose: {}
            )
        )
        #expect(panel.fittingSize == NSSize(width: 750, height: 600))
        store.dismissDocument()
        #expect(store.document == nil)
    }

    /// The release editor is a modal; a document it opens shows over it and
    /// closes back to it.
    @Test("A document opened from a modal leaves the modal open")
    func documentOverModal() {
        let store = UiStore()
        store.presentModal { Text(verbatim: "Edit release") }
        store.presentDocument(name: "rip.log", text: "Extraction log")
        #expect(!store.modalStack.isEmpty)
        #expect(
            store.document
                == OpenDocument(name: "rip.log", text: "Extraction log")
        )
        store.dismissDocument()
        #expect(!store.modalStack.isEmpty)
    }

    /// Change cover opens over the release editor; closing it leaves the
    /// editor, and closing the editor closes every modal.
    @Test("A modal opened from a modal stacks over it")
    func modalsStack() {
        let store = UiStore()
        store.presentModal { Text(verbatim: "Edit release") }
        store.presentModal { Text(verbatim: "Change cover") }
        #expect(store.modalStack.count == 2)
        store.dismissModal()
        #expect(store.modalStack.count == 1)
        store.presentModal { Text(verbatim: "Change cover") }
        store.dismissAllModals()
        #expect(store.modalStack.isEmpty)
    }
}
