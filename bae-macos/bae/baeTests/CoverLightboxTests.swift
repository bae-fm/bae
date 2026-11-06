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
        let file = releaseFile(
            PreviewData.previewArtFile("Lightbox fixture")
        )
        let bytes = try Data(contentsOf: URL(fileURLWithPath: path))
        let recorder = Reads()
        let images = ImageStore(fetchRemoteImage: { image, pixels in
            await recorder.read(Read(url: image.url, pixels: pixels))
            return bytes
        })
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
            // them, so it waits until both remote ones are read and laid out.
            try await Wait.until {
                Set(await recorder.reads.map(\.url)).count >= 2
            }
            try await SnapshotTestSupport.settle(host)
            try open(opening, window: window, host: host)
            // The keys below are for the lightbox, so they wait until it is
            // laid out.
            try await SnapshotTestSupport.settleUnmoved(host)
            // Return cannot save through the lightbox into the underlying picker.
            try HostedInput.keyEquivalent(.return, in: host)
            #expect(selected == nil)
            try HostedInput.keyDown(.rightArrow, in: window)
            try await SnapshotTestSupport.settleUnmoved(host)
            try HostedInput.keyDown(.escape, in: window)
            try await SnapshotTestSupport.settleUnmoved(host)
            #expect(!dismissed)
            #expect(selected == nil)
            try HostedInput.keyEquivalent(.return, in: host)
            try await SnapshotTestSupport.settleUnmoved(host)
            #expect(
                selected?.id == (opening == .thumbnail ? file.id : booklet.id)
            )
        }
    }

    @Test("Arrow keys browse the grid as soon as the sheet opens on it")
    func gridTakesKeysOnOpen() async throws {
        let front = remote("Front")
        let booklet = remote("Booklet")
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Booklet")
            )
        )
        let recorder = Reads()
        let images = ImageStore(fetchRemoteImage: { image, pixels in
            await recorder.read(Read(url: image.url, pixels: pixels))
            return bytes
        })
        var selected: CoverItem?
        let size = NSSize(width: 960, height: 700)
        try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                remoteItems: .linked([front, booklet]),
                releaseItems: [],
                selectedCover: front.selection,
                onSelect: { selected = $0 },
                onDone: {}
            )
            .environment(images)
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await Wait.until {
                Set(await recorder.reads.map(\.url)).count >= 2
            }
            try await SnapshotTestSupport.settle(host)
            // Nothing clicked or opened first: the grid has the keys.
            try HostedInput.keyDown(.rightArrow, in: window)
            try await SnapshotTestSupport.settle(host)
            try HostedInput.keyEquivalent(.return, in: host)
            try await SnapshotTestSupport.settle(host)
            #expect(selected?.id == booklet.id)
        }
    }

    /// Open the lightbox the way `opening` names.
    @MainActor
    private func open(
        _ opening: Opening,
        window: NSWindow,
        host: NSView
    ) throws {
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
    }

    @Test(
        "The lightbox stage reads the original; its strip asks for its own size"
    )
    func originalImageSource() async throws {
        let item = remote("Booklet")
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Booklet")
            )
        )
        let recorder = Reads()
        let images = ImageStore(fetchRemoteImage: { image, pixels in
            await recorder.read(Read(url: image.url, pixels: pixels))
            return bytes
        })
        let cursor = try #require(Cursor(items: [item, remote("Back")]))
        let size = NSSize(width: 800, height: 600)
        try await SnapshotTestSupport.withHostedWindow(
            LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
                .environment(images)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, _ in
            try await Wait.until { await recorder.reads.count >= 3 }
            let reads = await recorder.reads
            let booklet = "https://images.example/Booklet-original.png"
            let back = "https://images.example/Back-original.png"
            // The stage decodes the current image at its own resolution; the
            // strip's slots name their pixel size and core picks the copy.
            #expect(reads.contains(Read(url: booklet, pixels: nil)))
            #expect(reads.contains { $0.url == booklet && $0.pixels != nil })
            #expect(reads.contains { $0.url == back && $0.pixels != nil })
            #expect(!reads.contains(Read(url: back, pixels: nil)))
        }
    }

    private struct Read: Equatable {
        let url: String
        let pixels: UInt32?
    }

    private actor Reads {
        var reads: [Read] = []
        func read(_ read: Read) {
            reads.append(read)
            Wait.happened()
        }
    }

    private func releaseFile(_ file: BridgeFileVersion) -> CoverItem {
        CoverItem(
            coverChoice: BridgeCoverChoice(
                selection: .releaseImage(fileId: "scan-file"),
                image: .local(file: file)
            ),
            label: "scans/booklet.png"
        )
    }

    private func remote(_ name: String) -> CoverItem {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/\(name)-original.png",
            downscaled: [
                BridgeDownscaledCopy(
                    url: "https://images.example/\(name)-thumb.png",
                    maxEdge: 150
                )
            ]
        )
        return CoverItem(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .discogs
                    )
                ),
                image: .remote(image: image)
            ),
            label: name
        )
    }
}
