import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@MainActor
struct LibraryArtworkBrowserTests {
    @Test(
        "Library artwork opens on its cover and switches layouts without reloading or saving"
    )
    func libraryEntry() async throws {
        let art = remote("Booklet", source: .discogs)
        var lookups = 0
        var saved: BridgeCoverSelection?
        var dismissed = false
        let size = NSSize(width: 960, height: 700)
        try await withLibraryBrowser(
            size: size,
            fetch: {
                lookups += 1
                return .linked(covers: [art])
            },
            onSelect: { saved = $0 },
            onDone: { dismissed = true },
            body: { window, host in
                var observations = try await text(in: host, size: size)
                #expect(
                    labels(observations)
                        .carrying(String(localized: "Current Cover"))
                )
                #expect(
                    !labels(observations)
                        .carrying(String(localized: "Use This Cover"))
                )
                try click(
                    "Browse all images",
                    observations: observations,
                    window: window,
                    size: size
                )
                observations = try await text(in: host, size: size)
                #expect(
                    labels(observations).carrying(String(localized: "Images"))
                )
                #expect(saved == nil)
                // Advance the shared cursor to the provider booklet in grid mode.
                try HostedInput.keyDown(.rightArrow, in: window)
                try await SnapshotTestSupport.settle(host)
                try HostedInput.keyEquivalent(.space, in: host)
                observations = try await text(in: host, size: size)
                #expect(labels(observations).carrying("Discogs"))
                try HostedInput.keyEquivalent(.return, in: host)
                #expect(saved == nil)
                try click(
                    "Browse all images",
                    observations: observations,
                    window: window,
                    size: size
                )
                _ = try await text(in: host, size: size)
                #expect(lookups == 1)
                #expect(!dismissed)
                try HostedInput.keyEquivalent(.return, in: host)
                _ = try await text(in: host, size: size)
                #expect(saved == art.coverChoice.selection)
                #expect(dismissed)
            }
        )
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

    @Test(
        "The empty library lightbox remains navigable and exposes identification"
    )
    func emptyLightbox() async throws {
        var identified = false
        var dismissed = false
        let size = NSSize(width: 800, height: 520)
        try await SnapshotTestSupport.withHostedWindow(
            CoverGalleryView(
                remoteItems: .unlinked,
                releaseItems: [],
                selectedCover: nil,
                initialLayout: .lightbox,
                onFindRelease: { identified = true },
                onSelect: { _ in Issue.record("Empty artwork cannot be saved")
                },
                onDone: { dismissed = true }
            )
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            let observations = try await text(in: host, size: size)
            #expect(
                labels(observations)
                    .carrying(String(localized: "No linked release"))
            )
            try click(
                "Find release…",
                observations: observations,
                window: window,
                size: size
            )
            #expect(identified)
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
            _ = try await text(in: host, size: size)
            host.rootView = try view("v2")
            _ = try await text(in: host, size: size)
            #expect(await reads.versions == ["v1", "v2"])
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

    @Test(
        "The lightbox distinguishes loading, missing identity, empty results, and lookup failure",
        arguments: [
            RemoteCoverItems.loading([]), .unlinked, .linked([]),
            .failed([], message: "Lookup failed"),
        ]
    )
    func lookupStateInLightbox(_ remoteItems: RemoteCoverItems) async throws {
        let size = NSSize(width: 800, height: 520)
        try await SnapshotTestSupport.withHostedWindow(
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
            let visible = labels(try await text(in: host, size: size))
            #expect(
                visible.carrying(String(localized: "No remote covers found"))
                    == (remoteItems == .linked([]))
            )
            #expect(
                visible.carrying(String(localized: "No linked release"))
                    == (remoteItems == .unlinked)
            )
            #expect(
                visible.carrying(String(localized: "Fetching covers..."))
                    == remoteItems.isLoading
            )
            #expect(
                visible.carrying(String(localized: "No cover art available"))
                    == !remoteItems.isLoading
            )
            if case .failed = remoteItems {
                #expect(visible.carrying("Lookup failed"))
            }
        }
    }
}

extension LibraryArtworkBrowserTests {
    private func withLibraryBrowser<Value>(
        size: NSSize,
        fetch: @escaping () async throws -> BridgeRemoteCoverGallery,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void,
        body: (NSWindow, NSHostingView<AnyView>) async throws -> Value
    ) async throws -> Value {
        let release = PreviewData.releaseDetail(albumId: "a-01")
        release.summary.cover = BridgeImageRef(
            id: release.id,
            version: "cover-version",
            imageType: .cover
        )
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
                next: {
                    try await Task.sleep(for: .seconds(86_400))
                    throw CancellationError()
                },
                cancel: {}
            )
        })
        return try await SnapshotTestSupport.withHostedWindow(
            AnyView(
                CoverSheetView(
                    releaseId: release.id,
                    initialRelease: release,
                    initialLayout: .lightbox,
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
        func record(_ version: String) { versions.append(version) }
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

    private func labels(_ observations: [SnapshotTestSupport.RecognizedLine])
        -> [String]
    {
        observations.map(\.text)
    }

    private func text(in host: NSView, size: NSSize) async throws
        -> [SnapshotTestSupport.RecognizedLine]
    {
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        return try await SnapshotTestSupport.recognizedText(in: png)
    }

    /// Click where `label` was drawn.
    ///
    /// These buttons are drawn by SwiftUI: the AppKit control behind one
    /// carries no title and publishes no accessibility name, so where its
    /// words landed is the only handle on it. Matched by containment, because
    /// a button that draws a symbol beside its words comes back with the two
    /// glued together.
    private func click(
        _ label: String,
        observations: [SnapshotTestSupport.RecognizedLine],
        window: NSWindow,
        size: NSSize
    ) throws {
        let localized = String(localized: String.LocalizationValue(label))
            .replacingOccurrences(of: "…", with: "...")
        let observation = try #require(
            observations.first {
                $0.text
                    .replacingOccurrences(of: "…", with: "...")
                    .contains(localized)
            },
            "\(localized) is not among \(labels(observations))"
        )
        let point = NSPoint(
            x: observation.boundingBox.midX * size.width,
            y: observation.boundingBox.midY * size.height
        )
        try HostedInput.press(at: point, in: window)
    }
}
