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
        try await SnapshotTestSupport.withHostedWindow(
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
        ) { window, host in
            // The covers load after the first layout; the click is aimed at
            // them, so it waits until they are drawn.
            _ = try await SnapshotTestSupport.steadyBitmap(of: host, size: size)
            switch opening {
            case .preview:
                try HostedInput.click(
                    at: NSPoint(x: 720, y: 420),
                    in: window,
                    count: 1
                )
            case .thumbnail:
                try HostedInput.click(
                    at: NSPoint(x: 240, y: 480),
                    in: window,
                    count: 1
                )
                try HostedInput.click(
                    at: NSPoint(x: 240, y: 480),
                    in: window,
                    count: 2
                )
            case .space:
                try HostedInput.keyEquivalent(.space, in: host)
            }
            // The keys below are for the lightbox, so they wait until it is drawn.
            _ = try await SnapshotTestSupport.steadyBitmap(of: host, size: size)
            // Return cannot save through the lightbox into the underlying picker.
            try HostedInput.keyEquivalent(.return, in: host)
            #expect(selected == nil)
            try HostedInput.keyDown(.rightArrow, in: window)
            try await SnapshotTestSupport.settle(host)
            try HostedInput.keyDown(.escape, in: window)
            try await SnapshotTestSupport.settle(host)
            #expect(!dismissed)
            #expect(selected == nil)
            try HostedInput.keyEquivalent(.return, in: host)
            try await SnapshotTestSupport.settle(host)
            #expect(
                selected?.id == (opening == .thumbnail ? file.id : booklet.id)
            )
        }
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
        try await SnapshotTestSupport.withHostedWindow(
            LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
                .environment(images)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            for _ in 0..<100 {
                try await SnapshotTestSupport.settle(host)
                if await recorder.urls.count >= 3 { break }
            }
            let urls = await recorder.urls
            #expect(
                urls.contains("https://images.example/Booklet-original.png")
            )
            #expect(urls.contains("https://images.example/Booklet-thumb.png"))
            #expect(urls.contains("https://images.example/Back-thumb.png"))
            #expect(!urls.contains("https://images.example/Back-original.png"))
        }
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
}
