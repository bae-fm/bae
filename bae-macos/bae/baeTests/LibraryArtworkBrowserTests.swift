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
        let (window, host) = try hostLibraryBrowser(
            size: size,
            fetch: {
                lookups += 1
                return .linked(covers: [art])
            },
            onSelect: { saved = $0 },
            onDone: { dismissed = true }
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        var observations = try await text(in: host, size: size)
        #expect(
            labels(observations).contains(String(localized: "Current Cover"))
        )
        #expect(
            !labels(observations).contains(String(localized: "Use This Cover"))
        )
        try click(
            "Browse all images",
            observations: observations,
            window: window,
            size: size
        )
        observations = try await text(in: host, size: size)
        #expect(labels(observations).contains(String(localized: "Images")))
        #expect(saved == nil)
        // Advance the shared cursor to the provider booklet in grid mode.
        window.sendEvent(
            try key(
                window,
                String(try #require(UnicodeScalar(NSRightArrowFunctionKey))),
                code: 124
            )
        )
        await SnapshotTestSupport.settle(host)
        _ = host.performKeyEquivalent(with: try key(window, " ", code: 49))
        observations = try await text(in: host, size: size)
        #expect(labels(observations).contains("Discogs"))
        _ = host.performKeyEquivalent(with: try key(window, "\r", code: 36))
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
        _ = host.performKeyEquivalent(with: try key(window, "\r", code: 36))
        _ = try await text(in: host, size: size)
        #expect(saved == art.coverChoice.selection)
        #expect(dismissed)
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
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
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        let observations = try await text(in: host, size: size)
        #expect(
            labels(observations)
                .contains(String(localized: "No linked release"))
        )
        try click(
            "Find release…",
            observations: observations,
            window: window,
            size: size
        )
        #expect(identified)
        _ = host.performKeyEquivalent(with: try key(window, "\u{1b}", code: 53))
        #expect(dismissed)
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
            try view("v1"),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        _ = try await text(in: host, size: size)
        host.rootView = try view("v2")
        _ = try await text(in: host, size: size)
        #expect(await reads.versions == ["v1", "v2"])
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
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
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        let visible = labels(try await text(in: host, size: size))
        #expect(
            visible.contains(String(localized: "No remote covers found"))
                == (remoteItems == .linked([]))
        )
        #expect(
            visible.contains(String(localized: "No linked release"))
                == (remoteItems == .unlinked)
        )
        #expect(
            visible.contains(String(localized: "Fetching covers..."))
                == remoteItems.isLoading
        )
        #expect(
            visible.contains(String(localized: "No cover art available"))
                == !remoteItems.isLoading
        )
        if case .failed = remoteItems {
            #expect(visible.contains("Lookup failed"))
        }
    }
}

extension LibraryArtworkBrowserTests {
    private func hostLibraryBrowser(
        size: NSSize,
        fetch: @escaping () async throws -> BridgeRemoteCoverGallery,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void
    ) throws -> (NSWindow, NSHostingView<AnyView>) {
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
            fetchRemoteImage: { _ in bytes }
        )
        let library = Library(subscribeReleaseDetail: { _, _ in Subscription() }
        )
        return SnapshotTestSupport.hostInWindow(
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
        )
    }

    private actor CoverReads {
        var versions: [String] = []
        func record(_ version: String) { versions.append(version) }
    }

    private func remote(_ name: String, source: BridgeMetadataSource)
        -> BridgeRemoteCover
    {
        let url = "https://images.example/\(name).png"
        return BridgeRemoteCover(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        url: url,
                        source: source
                    )
                ),
                previewSource: .remote(url: url),
                thumbnailSource: .remote(url: url)
            ),
            label: name
        )
    }

    private final class Subscription: LiveSubscriptionProtocol,
        @unchecked Sendable
    {
        func cancel() {}
    }

    private func labels(_ observations: [VNRecognizedTextObservation])
        -> [String]
    {
        observations.compactMap { $0.topCandidates(1).first?.string }
    }

    private func text(in host: NSView, size: NSSize) async throws
        -> [VNRecognizedTextObservation]
    {
        let png = try await SnapshotTestSupport.capturePNG(
            host,
            size: size,
            waitNanoseconds: 200_000_000
        )
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        try VNImageRequestHandler(data: png, options: [:]).perform([request])
        return try #require(request.results)
    }

    private func click(
        _ label: String,
        observations: [VNRecognizedTextObservation],
        window: NSWindow,
        size: NSSize
    ) throws {
        let localized = String(localized: String.LocalizationValue(label))
            .replacingOccurrences(of: "…", with: "...")
        let observation = try #require(
            observations.first {
                $0.topCandidates(1).first?.string
                    .replacingOccurrences(of: "…", with: "...") == localized
            }
        )
        let point = NSPoint(
            x: observation.boundingBox.midX * size.width,
            y: observation.boundingBox.midY * size.height
        )
        let content = try #require(window.contentView)
        let control =
            SnapshotTestSupport.descendants(of: content)
            .compactMap { $0 as? NSControl }
            .first {
                $0.isEnabled && $0.convert($0.bounds, to: nil).contains(point)
            }
        // SwiftUI's native buttons are NSControl subclasses, not NSButton.
        // Exercise their action without a synthetic mouse-tracking loop.
        if let control {
            control.performClick(nil)
        }
        else {
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
                            clickCount: 1,
                            pressure: type == .leftMouseDown ? 1 : 0
                        )
                    )
                )
            }
        }
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
}
