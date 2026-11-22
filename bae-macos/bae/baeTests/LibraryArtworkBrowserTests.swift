import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct LibraryArtworkBrowserTests {
    /// Space opens the image the grid focuses, the release's cover, in the
    /// lightbox, and Escape brings the grid back with the keyboard on it,
    /// without looking the covers up again; Return then saves the image the
    /// right arrow focused.
    @Test("Library artwork saves the image picked in the grid")
    func gridSavesThePick() async throws {
        let art = remote("Booklet", source: .discogs)
        let delivered = DeliveredFocuses()
        var lookups = 0
        var saved: BridgeCoverSelection?
        var dismissed = false
        try await withLibraryBrowser(
            gallery: gallery(remote: [art]),
            delivered: delivered,
            calls: SheetCalls(
                lookup: {
                    lookups += 1
                    Wait.happened()
                },
                onSelect: { saved = $0 },
                onDone: {
                    dismissed = true
                    Wait.happened()
                }
            ),
            body: { window, host, lightbox in
                // Space needs the preview, which shows once the first read
                // arrives; the lookup can end before that read.
                try await Wait.until { lookups > 0 && delivered.count > 0 }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.space, in: host)
                try await Wait.until { lightbox.ended() }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyDown(.escape, in: window)
                try await SnapshotTestSupport.settle(host)
                #expect(!dismissed)
                try HostedInput.keyDown(.rightArrow, in: window)
                // The arrow asks core to focus the next cover; Return waits
                // for the read that answers.
                try await Wait.until {
                    delivered.delivered(
                        .remote(url: "https://images.example/Booklet.png")
                    )
                }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.return, in: host)
                try await Wait.until { dismissed }
                #expect(saved == art.coverChoice.selection)
                #expect(lookups == 1, "looked up \(lookups) times")
            }
        )
    }

    @Test("An updated library cover reloads in the open lightbox")
    func updatedCover() async throws {
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Cover")
            )
        )
        let reads = CoverReads()
        let images = ImageStore(fetchReleaseImageBytes: { _, source in
            if case .cover(let image) = source {
                await reads.record(image.version)
            }
            return bytes
        })
        let size = NSSize(width: 800, height: 600)
        let lightbox = LightboxImageLoad()
        func view(_ version: String) throws -> some View {
            let cover = savedCover(version: version)
            return MacLightbox(
                place: LightboxPlace(
                    items: [cover],
                    current: cover,
                    neighbors: nil
                ),
                load: lightbox,
                onSelect: { _ in },
                onDismiss: {}
            )
            .environment(images).frame(width: size.width, height: size.height)
        }
        try await SnapshotTestSupport.withHostedWindow(
            try view("v1"),
            size: size
        ) { _, host in
            try await Wait.until { await reads.versions == ["v1"] }
            host.rootView = try view("v2")
            try await Wait.until { await reads.versions == ["v1", "v2"] }
        }
    }
}

extension LibraryArtworkBrowserTests {
    private func withLibraryBrowser<Value>(
        gallery: BridgeCoverGallery,
        delivered: DeliveredFocuses? = nil,
        calls: SheetCalls,
        body:
            (NSWindow, NSHostingView<AnyView>, LightboxImageLoad) async throws
            -> Value
    ) async throws -> Value {
        let size = NSSize(width: 960, height: 700)
        let releaseId = "release"
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Booklet")
            )
        )
        let images = ImageStore(
            fetchReleaseImageBytes: { _, _ in bytes },
            fetchRemoteImage: { _, _ in bytes }
        )
        let lightbox = LightboxImageLoad()
        return try await SnapshotTestSupport.withHostedWindow(
            AnyView(
                CoverSheetView(
                    owner: .release(releaseId: releaseId),
                    lightboxLoad: lightbox,
                    refreshCovers: calls.lookup,
                    onSelect: calls.onSelect,
                    onDone: calls.onDone
                )
                .environment(images)
                .environment(
                    Importer(coverGallery: {
                        .fixed(gallery, delivered: delivered)
                    })
                )
                .frame(width: size.width, height: size.height)
            ),
            size: size
        ) {
            try await body($0, $1, lightbox)
        }
    }

    /// What the sheet calls: the lookup of the catalogs' covers, saving the
    /// cover picked, and leaving.
    struct SheetCalls {
        var lookup: () async throws -> Void
        var onSelect: (BridgeCoverSelection) async throws -> Void
        var onDone: () -> Void
    }

    private actor CoverReads {
        var versions: [String] = []
        func record(_ version: String) {
            versions.append(version)
            Wait.happened()
        }
    }

    private func remote(_ name: String, source: BridgeCatalog)
        -> BridgeRemoteCover
    {
        let image = BridgeRemoteImageSet(
            url: "https://images.example/\(name).png",
            downscaled: []
        )
        return BridgeRemoteCover(
            coverChoice: BoardArtwork.catalogCoverChoice(image, source: source),
            sourceName: source == .discogs ? "Discogs" : "Cover Art Archive",
            label: .lead(releaseGroup: false)
        )
    }
}

extension LibraryArtworkBrowserTests {
    /// The release's saved cover, at `version`.
    private func savedCover(version: String = "v1") -> CoverItem {
        CoverItem(gallery: savedGalleryCover(version: version))
    }

    private func savedGalleryCover(version: String = "v1")
        -> BridgeGalleryCover
    {
        BridgeGalleryCover(
            key: .saved,
            image: .releaseImage(
                releaseId: "release",
                source: .cover(
                    image: BridgeImageRef(
                        id: "release",
                        version: version,
                        imageType: .cover
                    )
                )
            ),
            label: .current
        )
    }

    private func galleryCover(_ cover: BridgeRemoteCover) -> BridgeGalleryCover
    {
        let url =
            switch cover.coverChoice.image {
            case .remote(let image): image.url
            default: preconditionFailure("a catalog's cover has a remote image")
            }
        return BridgeGalleryCover(
            key: .remote(url: url),
            image: cover.coverChoice.image,
            label: .remote(
                sourceName: cover.sourceName,
                label: cover.label,
                selection: cover.coverChoice.selection
            )
        )
    }

    /// What core reads for the release under `filter`: its saved cover (under
    /// all sources only) and the remote covers given.
    private func gallery(
        remote covers: [BridgeRemoteCover],
        current: BridgeGalleryCover? = nil,
        filter: ArtworkBrowserState.Filter = .all
    ) -> BridgeCoverGallery {
        let saved = current ?? (filter == .all ? savedGalleryCover() : nil)
        return BridgeCoverGallery(
            current: saved,
            fetch: .linked,
            remote: BridgeCoverSection(
                items: covers.map(galleryCover),
                count: UInt32(covers.count)
            ),
            files: nil,
            total: UInt32(covers.count + (saved == nil ? 0 : 1)),
            focus: nil
        )
    }
}
