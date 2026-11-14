import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct LibraryArtworkBrowserTests {
    /// Library artwork opens in the lightbox, where Return saves nothing and
    /// Escape leaves without saving. The lightbox's words are the
    /// `artwork-lightbox` shot's.
    @Test("Library artwork opens in the lightbox without saving")
    func libraryEntry() async throws {
        let art = remote("Booklet", source: .discogs)
        var lookups = 0
        var saved: BridgeCoverSelection?
        var dismissed = false
        try await withLibraryBrowser(
            layout: .lightbox,
            gallery: gallery(remote: [art]),
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
                // The release's cover, which the lightbox opens on.
                try await Wait.until { lightbox.ended() }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.return, in: host)
                try await SnapshotTestSupport.settle(host)
                #expect(saved == nil)
                try HostedInput.keyDown(.escape, in: window)
                try await Wait.until { dismissed }
                #expect(saved == nil)
                #expect(lookups == 1)
            }
        )
    }

    /// Space opens the image the grid's cursor is on, the release's cover, in
    /// the lightbox, and Escape brings the grid back with the keyboard on it,
    /// without looking the covers up again; Return then saves the image the
    /// cursor moved to.
    @Test("Library artwork saves the image picked in the grid")
    func gridSavesThePick() async throws {
        let art = remote("Booklet", source: .discogs)
        var lookups = 0
        var saved: BridgeCoverSelection?
        var dismissed = false
        try await withLibraryBrowser(
            layout: .grid,
            gallery: gallery(remote: [art]),
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
                try await Wait.until { lookups > 0 }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.space, in: host)
                try await Wait.until { lightbox.ended() }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyDown(.escape, in: window)
                try await SnapshotTestSupport.settle(host)
                #expect(!dismissed)
                try HostedInput.keyDown(.rightArrow, in: window)
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.return, in: host)
                try await Wait.until { dismissed }
                #expect(saved == art.coverChoice.selection)
                #expect(lookups == 1, "looked up \(lookups) times")
            }
        )
    }

    /// Browse all images, in the lightbox, switches the browser to its grid
    /// with the image shown still the cursor's; where that button sits is no
    /// longer tested.
    @Test("Browsing all images from the lightbox keeps the image shown")
    func browsingAllKeepsTheCursor() {
        let cover = savedCover()
        let booklet = CoverItem(
            gallery: galleryCover(remote("Booklet", source: .discogs))
        )
        var browser = ArtworkBrowserState(layout: .lightbox)
        browser.update(
            content: CoverGalleryContent(
                gallery(remote: [remote("Booklet", source: .discogs)])
            ),
            selectedCover: nil
        )
        browser.cursor?.select(id: booklet.id)
        browser.layout = .grid
        #expect(browser.cursor?.current.id == booklet.id)
        #expect(browser.cursor?.items.first == cover)
    }

    /// Core reads the gallery again for each source, so the content the
    /// browser is handed changes under its cursor: the cursor stays on the
    /// cover it was on while the new content holds it, else goes to the
    /// first.
    @Test("Arriving artwork preserves selection across the sources")
    func arrivalsAndFilters() throws {
        let discogs = remote("Booklet", source: .discogs)
        let archive = remote("Back", source: .musicBrainz)
        let cover = savedCover()
        var browser = ArtworkBrowserState(layout: .lightbox)
        browser.update(
            content: CoverGalleryContent(gallery(remote: [])),
            selectedCover: nil
        )
        #expect(browser.cursor?.current.id == cover.id)
        browser.update(
            content: CoverGalleryContent(gallery(remote: [discogs, archive])),
            selectedCover: nil
        )
        #expect(browser.cursor?.current.id == cover.id)
        let archiveItem = CoverItem(gallery: galleryCover(archive))
        browser.cursor?.select(id: archiveItem.id)
        browser.layout = .grid
        browser.update(
            content: CoverGalleryContent(
                gallery(remote: [archive], current: nil, filter: .musicBrainz)
            ),
            selectedCover: nil
        )
        #expect(browser.cursor?.items == [archiveItem])
        browser.layout = .lightbox
        browser.update(
            content: CoverGalleryContent(gallery(remote: [discogs, archive])),
            selectedCover: nil
        )
        #expect(browser.cursor?.current.id == archiveItem.id)
        browser.update(
            content: CoverGalleryContent(
                gallery(remote: [], current: nil, filter: .releaseFiles)
            ),
            selectedCover: nil
        )
        #expect(browser.cursor == nil)
        #expect(cover.selection == nil)
    }

    /// The empty lightbox offers to find the release (the
    /// `artwork-lightbox-unlinked` shot); its Find release… runs
    /// `onFindRelease` as is, so where that button sits is no longer tested.
    @Test("The empty library lightbox can be left")
    func emptyLightbox() async throws {
        var dismissed = false
        let size = NSSize(width: 800, height: 520)
        try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                content: .empty,
                remoteItems: .unlinked,
                filter: .constant(.all),
                selectedCover: nil,
                lightboxLoad: LightboxImageLoad(),
                initialLayout: .lightbox,
                onFindRelease: {},
                onSelect: { _ in Issue.record("Empty artwork cannot be saved")
                },
                onDone: { dismissed = true }
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            try HostedInput.keyEquivalent(.escape, in: host)
            #expect(dismissed)
        }
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
            let cursor = try #require(Cursor(items: [cover]))
            return LightboxView(
                cursor: cursor,
                load: lightbox,
                onUpdate: { _ in },
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
    @Test(
        "The release-file filter retains library files and excludes provider artwork"
    )
    func releaseFileFilter() {
        let file = CoverItem(
            gallery: BridgeGalleryCover(
                selection: .releaseImage(fileId: "booklet-file"),
                image: .releaseImage(
                    releaseId: "release",
                    source: .releaseFile(fileId: "booklet-file")
                ),
                label: .file(name: "booklet.png")
            )
        )
        let remote = CoverItem(
            coverChoice: remote("Back", source: .discogs).coverChoice,
            label: "Back"
        )
        let content = CoverGalleryContent(
            remote: [remote],
            files: [file],
            filter: .releaseFiles
        )
        #expect(content.items == [file])
        #expect(content.files?.items == [file])
        #expect(content.remote == nil)
        #expect(content.total == 1)
    }

    /// What the lightbox says of each state is the `artwork-lightbox-fetching`,
    /// `-unlinked`, `-none-found` and `-lookup-failed` shots'. The grid is
    /// laid out behind it with its saving spinner in every state, so a
    /// lookup still out spins more than the rest.
    @Test("The lightbox spins only while its lookup is out")
    func lookupStateInLightbox() async throws {
        let settled = [
            RemoteCoverItems.unlinked, .linked([]),
            .failed([], failure: DisplayError(line: "Lookup failed")),
        ]
        let resting = try await lightboxSpinners(.linked([]))
        for remoteItems in settled {
            #expect(
                try await lightboxSpinners(remoteItems) == resting,
                "\(remoteItems)"
            )
        }
        #expect(try await lightboxSpinners(.loading([])) > resting)
    }

    private func lightboxSpinners(_ remoteItems: RemoteCoverItems)
        async throws -> Int
    {
        let size = NSSize(width: 800, height: 520)
        return try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                content: .empty,
                remoteItems: remoteItems,
                filter: .constant(.all),
                selectedCover: nil,
                lightboxLoad: LightboxImageLoad(),
                initialLayout: .lightbox,
                onSelect: { _ in },
                onDone: {}
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
                .count
        }
    }
}

extension LibraryArtworkBrowserTests {
    private func withLibraryBrowser<Value>(
        layout: ArtworkBrowserState.Layout,
        gallery: BridgeCoverGallery,
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
                    releaseId: releaseId,
                    lightboxLoad: lightbox,
                    initialLayout: layout,
                    refreshCovers: calls.lookup,
                    onSelect: calls.onSelect,
                    onDone: calls.onDone
                )
                .environment(images)
                .environment(Library(coverGallery: { .fixed(gallery) }))
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
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: source
                    )
                ),
                image: .remote(image: image)
            ),
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
            selection: nil,
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
        BridgeGalleryCover(
            selection: cover.coverChoice.selection,
            image: cover.coverChoice.image,
            label: .remote(sourceName: cover.sourceName, label: cover.label)
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
            sections: [
                BridgeCoverSection(
                    source: .remote,
                    items: covers.map(galleryCover),
                    count: UInt32(covers.count)
                )
            ],
            total: UInt32(covers.count + (saved == nil ? 0 : 1))
        )
    }
}

/// A picker's live read that answers each request with `gallery`.
extension CoverGalleryQuery {
    static func fixed(_ gallery: BridgeCoverGallery) -> CoverGalleryQuery {
        let (stream, continuation) = AsyncStream<BridgeCoverGallerySnapshot>
            .makeStream()
        let pulls = FixedPulls(stream.makeAsyncIterator())
        return CoverGalleryQuery(
            setRequest: { releaseId, filter in
                continuation.yield(
                    BridgeCoverGallerySnapshot(
                        request: BridgeCoverGalleryRequest(
                            releaseId: releaseId,
                            filter: filter
                        ),
                        gallery: gallery
                    )
                )
            },
            values: PulledValues(
                read: { try await pulls.next() },
                cancel: { continuation.finish() }
            )
        )
    }
}

/// The one reader of a stream, which waits for each request's value in turn.
private final class FixedPulls: @unchecked Sendable {
    private var iterator: AsyncStream<BridgeCoverGallerySnapshot>.AsyncIterator

    init(_ iterator: AsyncStream<BridgeCoverGallerySnapshot>.AsyncIterator) {
        self.iterator = iterator
    }

    func next() async throws -> BridgeCoverGallerySnapshot {
        guard let snapshot = await iterator.next() else {
            throw CancellationError()
        }
        return snapshot
    }
}
