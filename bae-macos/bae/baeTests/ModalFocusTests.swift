import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// While a sheet, the document viewer or a lightbox is open, Tab and
/// VoiceOver reach only what it holds, as they do for an alert. Each test
/// puts two named buttons in the window under the layer.
@MainActor
@Suite("Modal focus", .serialized)
struct ModalFocusTests {
    private static let underneath: Set<String> = ["Under one", "Under two"]

    /// What the host window shows over its two buttons.
    @MainActor
    @Observable
    final class Presented {
        var layers: [MacLayer] = []
        var systemSheet = false
        var alert = false
    }

    struct Host: View {
        @Bindable
        var presented: Presented
        let images: ImageStore

        var body: some View {
            VStack {
                Button("Under one") {}
                Button("Under two") {}
            }
            .frame(width: 600, height: 400)
            .macLayers(presented.layers)
            .sheet(isPresented: $presented.systemSheet) {
                MacSheet(
                    "System sheet",
                    width: 300,
                    content: { Button("Inside") {} },
                    footer: { EmptyView() }
                )
            }
            .macAlert(isPresented: $presented.alert) {
                (
                    MacAlertWords(Text(verbatim: "Alert")),
                    [MacAlertButton(Text(verbatim: "OK"))]
                )
            }
            .layerEnvironment { $0.environment(images) }
        }
    }

    private func images() throws -> ImageStore {
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Lightbox fixture")
            )
        )
        return CoverLightboxTests()
            .imageStore(
                serving: bytes,
                recording: CoverLightboxTests.Reads()
            )
    }

    /// Hosts the two buttons and runs `body` with the window and what it
    /// shows.
    func withHost(
        _ body: (NSWindow, NSView, Presented) async throws -> Void
    ) async throws {
        let presented = Presented()
        let size = NSSize(width: 600, height: 400)
        try await SnapshotTestSupport.withHostedWindow(
            Host(presented: presented, images: try images()),
            size: size
        ) { window, host in
            // A failed test closes what it opened too, since the next test
            // reads the stack that every window shares.
            var failure: (any Error)?
            do {
                try await SnapshotTestSupport.settle(host)
                try await body(window, host, presented)
            }
            catch { failure = error }
            presented.layers = []
            presented.systemSheet = false
            try await SnapshotTestSupport.untilSheet(
                on: window,
                attached: false
            )
            presented.alert = false
            try await LayerPanels.closed()
            if let failure { throw failure }
        }
    }

    /// The window that holds the keyboard: the top panel, or the sheet
    /// attached to `window`.
    private func keyed(_ window: NSWindow) -> NSWindow {
        ModalWindows.shared.panels.last ?? window.attachedSheet ?? window
    }

    /// Whether `window` holds a focused element named one of `names`.
    private func focused(in window: NSWindow) throws -> Set<String> {
        let tree = try AccessibilityTree.read(of: window, subject: "a window")
        return Set(
            tree.nodes.filter { $0.focused == true }.compactMap(\.controlName)
        )
    }

    /// Checks what Tab and VoiceOver reach with the layer open.
    private func checkFocusInside(
        _ window: NSWindow,
        host: NSView,
        layer: String
    ) async throws {
        let top = keyed(window)
        #expect(top !== window, "\(layer): something is over the window")
        #expect(
            autoreleasepool { top.isAccessibilityModal() },
            "\(layer): the key window is accessibility-modal"
        )
        let tree = try AccessibilityTree.read(of: top, subject: layer)
        #expect(
            tree.nodes.contains { $0.controlName != nil },
            "\(layer): the key window holds its own controls"
        )
        #expect(
            Set(tree.nodes.compactMap(\.controlName))
                .isDisjoint(with: Self.underneath),
            "\(layer): VoiceOver's tree holds a control of the window under it"
        )
        let content = top.contentView ?? host
        for _ in 0..<12 {
            HostedInput.selectNextKey(in: top)
            try await SnapshotTestSupport.settle(content)
            #expect(
                try focused(in: top).isDisjoint(with: Self.underneath),
                "\(layer): Tab reached a control of the window under it"
            )
            #expect(
                try focused(in: window).isDisjoint(with: Self.underneath),
                "\(layer): the window under it holds the focus"
            )
        }
    }

    func sheetLayer() -> MacLayer {
        MacLayer(id: "sheet") {
            MacModal(onDismiss: {}) {
                VStack {
                    Text(verbatim: "Edit release")
                    Button("Inside") {}
                }
                .frame(width: 300, height: 200)
            }
        }
    }

    func documentLayer() -> MacLayer {
        MacLayer(id: "document") {
            MacModal(onDismiss: {}) {
                DocumentViewerView(name: "rip.log", text: "Log", onClose: {})
            }
        }
    }

    private func folderLightboxLayer() -> MacLayer {
        let tests = CoverLightboxTests()
        let front = tests.remote("Front")
        let back = tests.remote("Back")
        return MacLayer(id: "lightbox", fades: true) {
            MacLightbox(
                place: LightboxPlace(
                    items: [front, back],
                    current: front,
                    neighbors: LightboxNeighbors(
                        previous: back.id,
                        next: back.id
                    )
                ),
                load: LightboxImageLoad(),
                onSelect: { _ in },
                onDismiss: {}
            )
        }
    }

    /// The Change cover sheet, which opens on its lightbox when
    /// `opensOnLightbox`.
    private func changeCoverLayer(opensOnLightbox: Bool) -> MacLayer {
        let tests = CoverLightboxTests()
        let front = tests.remote("Front")
        let booklet = tests.remote("Booklet")
        return MacLayer(id: "change cover") {
            MacModal(onDismiss: {}) {
                FocusingCoverGallery(
                    gallery: tests.gallery(remote: [front, booklet], files: [])
                ) { content, focusedCover in
                    MacCoverGallery(
                        content: content,
                        remoteItems: .linked([front, booklet]),
                        filter: .constant(.all),
                        focusedCover: focusedCover,
                        selectedCover: front.selection,
                        lightboxLoad: LightboxImageLoad(),
                        opensOnLightbox: opensOnLightbox,
                        onSelect: { _ in },
                        onDone: {}
                    )
                }
                .frame(width: 960, height: 600)
            }
        }
    }

    @Test("A sheet in the window keeps Tab and VoiceOver inside")
    func sheet() async throws {
        try await withHost { window, host, presented in
            presented.layers = [sheetLayer()]
            _ = try await LayerPanels.top()
            try await checkFocusInside(window, host: host, layer: "sheet")
        }
    }

    @Test("The document viewer keeps Tab and VoiceOver inside")
    func document() async throws {
        try await withHost { window, host, presented in
            presented.layers = [documentLayer()]
            _ = try await LayerPanels.top()
            try await checkFocusInside(window, host: host, layer: "document")
        }
    }

    @Test("The folder lightbox keeps Tab and VoiceOver inside")
    func folderLightbox() async throws {
        try await withHost { window, host, presented in
            presented.layers = [folderLightboxLayer()]
            _ = try await LayerPanels.top()
            try await checkFocusInside(window, host: host, layer: "lightbox")
        }
    }

    @Test("The Change cover sheet keeps Tab and VoiceOver inside")
    func changeCover() async throws {
        try await withHost { window, host, presented in
            presented.layers = [changeCoverLayer(opensOnLightbox: false)]
            _ = try await LayerPanels.top()
            try await checkFocusInside(window, host: host, layer: "cover")
        }
    }

    @Test("The Change cover sheet's lightbox keeps Tab and VoiceOver inside")
    func changeCoverLightbox() async throws {
        try await withHost { window, host, presented in
            presented.layers = [changeCoverLayer(opensOnLightbox: true)]
            _ = try await LayerPanels.top(count: 2)
            try await checkFocusInside(
                window,
                host: host,
                layer: "cover lightbox"
            )
        }
    }

    /// Whether `condition` holds within the hang guard.
    func holds(
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () -> Bool
    ) async -> Bool {
        do {
            try await SnapshotTestSupport.untilWindows(
                file: file,
                line: line,
                condition
            )
            return true
        }
        catch { return false }
    }
}
