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
            fetch: {
                lookups += 1
                Wait.happened()
                return .linked(covers: [art])
            },
            onSelect: { saved = $0 },
            onDone: {
                dismissed = true
                Wait.happened()
            },
            body: { window, host in
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
            fetch: {
                lookups += 1
                Wait.happened()
                return .linked(covers: [art])
            },
            onSelect: { saved = $0 },
            onDone: {
                dismissed = true
                Wait.happened()
            },
            body: { window, host in
                try await Wait.until { lookups > 0 }
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.space, in: host)
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
        let cover = CoverItem(
            releaseId: "release",
            cover: BridgeImageRef(
                id: "release",
                version: "v1",
                imageType: .cover
            )
        )
        let booklet = CoverItem(
            coverChoice: remote("Booklet", source: .discogs).coverChoice,
            label: "Booklet"
        )
        var browser = ArtworkBrowserState(layout: .lightbox)
        browser.update(
            currentCover: cover,
            remoteItems: [booklet],
            releaseItems: [],
            selectedCover: nil
        )
        browser.cursor?.select(id: booklet.id)
        browser.layout = .grid
        #expect(browser.cursor?.current.id == booklet.id)
        #expect(browser.currentCover == cover)
    }

    @Test(
        "Arriving artwork preserves selection; source filters share the lightbox cursor"
    )
    func arrivalsAndFilters() throws {
        let cover = CoverItem(
            releaseId: "release",
            cover: BridgeImageRef(
                id: "release",
                version: "v1",
                imageType: .cover
            )
        )
        let discogs = CoverItem(
            coverChoice: remote("Booklet", source: .discogs).coverChoice,
            label: "Booklet"
        )
        let archive = CoverItem(
            coverChoice: remote("Back", source: .musicBrainz).coverChoice,
            label: "Back"
        )
        var browser = ArtworkBrowserState(layout: .lightbox)
        browser.update(
            currentCover: cover,
            remoteItems: [],
            releaseItems: [],
            selectedCover: nil
        )
        #expect(browser.cursor?.current.id == cover.id)
        browser.update(
            currentCover: cover,
            remoteItems: [discogs, archive],
            releaseItems: [],
            selectedCover: nil
        )
        #expect(browser.cursor?.current.id == cover.id)
        browser.cursor?.select(id: archive.id)
        browser.layout = .grid
        browser.setFilter(.musicBrainz)
        #expect(browser.cursor?.items == [archive])
        #expect(browser.remoteItems == [archive])
        #expect(browser.currentCover == nil)
        browser.layout = .lightbox
        browser.setFilter(.all)
        #expect(browser.cursor?.current.id == archive.id)
        browser.setFilter(.releaseFiles)
        #expect(browser.cursor == nil)
        browser.setFilter(.discogs)
        #expect(browser.cursor?.current.id == discogs.id)
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
                remoteItems: .unlinked,
                releaseItems: [],
                selectedCover: nil,
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
        func view(_ version: String) throws -> some View {
            let cover = CoverItem(
                releaseId: "release",
                cover: BridgeImageRef(
                    id: "release",
                    version: version,
                    imageType: .cover
                )
            )
            let cursor = try #require(Cursor(items: [cover]))
            return LightboxView(
                cursor: cursor,
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
            releaseId: "release",
            file: BridgeFile(
                id: "booklet-file",
                originalFilename: "booklet.png",
                name: "booklet.png",
                folder: nil,
                kind: .image(format: "PNG"),
                fileSize: 100,
                contentType: "image/png",
                isImage: true,
                audioFormat: nil
            )
        )
        let remote = CoverItem(
            coverChoice: remote("Back", source: .discogs).coverChoice,
            label: "Back"
        )
        var browser = ArtworkBrowserState(layout: .grid)
        browser.update(
            currentCover: nil,
            remoteItems: [remote],
            releaseItems: [file],
            selectedCover: nil
        )
        browser.setFilter(.releaseFiles)
        #expect(browser.cursor?.items == [file])
        #expect(browser.releaseItems == [file])
        #expect(browser.remoteItems.isEmpty)
        #expect(!browser.showsRemoteSources)
        #expect(browser.showsReleaseFiles)
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
                remoteItems: remoteItems,
                releaseItems: [],
                selectedCover: nil,
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
        fetch: @escaping () async throws -> BridgeRemoteCoverGallery,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void,
        body: (NSWindow, NSHostingView<AnyView>) async throws -> Value
    ) async throws -> Value {
        let size = NSSize(width: 960, height: 700)
        let album = try #require(PreviewData.albumDetails["a-01"])
        var bridge = try #require(
            album.releases.first { $0.id == album.album.primaryReleaseId }
        )
        bridge.cover = BridgeImageRef(
            id: bridge.id,
            version: "cover-version",
            imageType: .cover
        )
        let release = ReleaseDetail(bridge: bridge)
        let bytes = try Data(
            contentsOf: URL(
                fileURLWithPath: PreviewData.previewArtPath("Booklet")
            )
        )
        let images = ImageStore(
            fetchReleaseImageBytes: { _, _ in bytes },
            fetchRemoteImage: { _, _ in bytes }
        )
        // A read that never answers: the sheet shows the release it was
        // opened with.
        let library = Library(releaseDetail: {
            DetailQuery(
                setId: { _ in },
                values: PulledValues(read: { try await Task.neverAnswers() })
            )
        })
        return try await SnapshotTestSupport.withHostedWindow(
            AnyView(
                CoverSheetView(
                    releaseId: release.id,
                    initialRelease: release,
                    initialLayout: layout,
                    fetchRemoteCovers: fetch,
                    onSelect: onSelect,
                    onDone: onDone
                )
                .environment(library).environment(images)
                .frame(width: size.width, height: size.height)
            ),
            size: size
        ) {
            try await body($0, $1)
        }
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
            label: name
        )
    }
}
