import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct CoverLightboxTests {
    enum Opening: CaseIterable {
        case preview, thumbnail, space
    }

    @Test(
        "Browsing artwork does not apply a cover or dismiss its picker",
        arguments: Opening.allCases
    )
    func browseWithoutSaving(_ opening: Opening) async throws {
        let front = remote("Front")
        let booklet = remote("Booklet")
        let path = PreviewData.previewArtPath("Lightbox fixture")
        let file = releaseFile(path)
        let bytes = try Data(contentsOf: URL(fileURLWithPath: path))
        let images = ImageStore(fetchRemoteImage: { _ in bytes })
        var selected: CoverItem?
        var dismissed = false
        let size = NSSize(width: 960, height: 700)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            CoverGalleryView(
                remoteItems: .linked([front, booklet]),
                releaseItems: [file],
                selectedCover: front.selection,
                onSelect: { selected = $0 },
                onDone: { dismissed = true }
            )
            .environment(images)
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        try await Task.sleep(for: .milliseconds(100))
        switch opening {
        case .preview:
            try click(window, at: NSPoint(x: 720, y: 420), count: 1)
        case .thumbnail:
            try click(window, at: NSPoint(x: 240, y: 480), count: 1)
            try click(window, at: NSPoint(x: 240, y: 480), count: 2)
        case .space:
            _ = host.performKeyEquivalent(with: try key(window, " ", code: 49))
        }
        try await Task.sleep(for: .milliseconds(100))
        await SnapshotTestSupport.settle(host)
        // Return cannot save through the lightbox into the underlying picker.
        _ = host.performKeyEquivalent(with: try key(window, "\r", code: 36))
        #expect(selected == nil)
        window.sendEvent(
            try key(
                window,
                String(try #require(UnicodeScalar(NSRightArrowFunctionKey))),
                code: 124
            )
        )
        await SnapshotTestSupport.settle(host)
        window.sendEvent(try key(window, "\u{1b}", code: 53))
        await SnapshotTestSupport.settle(host)
        #expect(!dismissed)
        #expect(selected == nil)
        _ = host.performKeyEquivalent(with: try key(window, "\r", code: 36))
        await SnapshotTestSupport.settle(host)
        #expect(selected?.id == (opening == .thumbnail ? file.id : booklet.id))
    }

    @Test(
        "The lightbox reads the original and keeps the provider thumbnail separate"
    )
    func originalImageSource() async throws {
        let item = remote("Booklet")
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Booklet")
            )
        )
        let recorder = Reads()
        let images = ImageStore(fetchRemoteImage: { url in
            await recorder.read(url)
            return bytes
        })
        let cursor = try #require(Cursor(items: [item, remote("Back")]))
        let size = NSSize(width: 800, height: 600)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
                .environment(images)
                .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        for _ in 0..<100 {
            await SnapshotTestSupport.settle(host)
            if await recorder.urls.count >= 3 { break }
        }
        let urls = await recorder.urls
        #expect(urls.contains("https://images.example/Booklet-original.png"))
        #expect(urls.contains("https://images.example/Booklet-thumb.png"))
        #expect(urls.contains("https://images.example/Back-thumb.png"))
        #expect(!urls.contains("https://images.example/Back-original.png"))
    }

    private actor Reads {
        var urls: [String] = []
        func read(_ url: String) { urls.append(url) }
    }

    private func releaseFile(_ path: String) -> CoverItem {
        CoverItem(
            coverChoice: BridgeCoverChoice(
                selection: .releaseImage(fileId: "scan-file"),
                previewSource: .local(path: path),
                thumbnailSource: .local(path: path)
            ),
            label: "scans/booklet.png"
        )
    }

    private func remote(_ name: String) -> CoverItem {
        let original = "https://images.example/\(name)-original.png"
        return CoverItem(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        url: original,
                        source: .discogs
                    )
                ),
                previewSource: .remote(url: original),
                thumbnailSource: .remote(
                    url: "https://images.example/\(name)-thumb.png"
                )
            ),
            label: name
        )
    }

    private func key(_ window: NSWindow, _ characters: String, code: UInt16)
        throws -> NSEvent
    {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber,
                context: nil,
                characters: characters,
                charactersIgnoringModifiers: characters,
                isARepeat: false,
                keyCode: code
            )
        )
    }

    private func click(_ window: NSWindow, at point: NSPoint, count: Int) throws
    {
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type,
                        location: point,
                        modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime,
                        windowNumber: window.windowNumber,
                        context: nil,
                        eventNumber: 0,
                        clickCount: count,
                        pressure: type == .leftMouseDown ? 1 : 0
                    )
                )
            )
        }
    }
}
