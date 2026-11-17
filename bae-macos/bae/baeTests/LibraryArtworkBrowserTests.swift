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
            layout: .grid,
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
                focusedCover: .constant(nil),
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
            return LightboxView(
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
                focusedCover: .constant(nil),
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
                    initialLayout: layout,
                    refreshCovers: calls.lookup,
                    onSelect: calls.onSelect,
                    onDone: calls.onDone
                )
                .environment(images)
                .environment(
                    Library(coverGallery: {
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
            key: .saved,
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
        let url =
            switch cover.coverChoice.image {
            case .remote(let image): image.url
            default: preconditionFailure("a catalog's cover draws its image")
            }
        return BridgeGalleryCover(
            key: .remote(url: url),
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
            total: UInt32(covers.count + (saved == nil ? 0 : 1)),
            focus: nil
        )
    }
}

/// A picker's live read that answers each request with `gallery`.
extension CoverGalleryQuery {
    /// `delivered`, when given, records the focus each value answers.
    static func fixed(
        _ gallery: BridgeCoverGallery,
        delivered: DeliveredFocuses? = nil
    ) -> CoverGalleryQuery {
        let (stream, continuation) = AsyncStream<BridgeCoverGallerySnapshot>
            .makeStream()
        let pulls = FixedPulls(stream.makeAsyncIterator())
        return CoverGalleryQuery(
            setRequest: { owner, filter, focus in
                continuation.yield(
                    BridgeCoverGallerySnapshot(
                        request: BridgeCoverGalleryRequest(
                            owner: owner,
                            filter: filter,
                            focus: focus
                        ),
                        gallery: gallery.focused(on: focus)
                    )
                )
            },
            values: PulledValues(
                read: {
                    let snapshot = try await pulls.next()
                    delivered?.record(snapshot.request?.focus)
                    return snapshot
                },
                cancel: { continuation.finish() }
            )
        )
    }
}

/// The focus each value of a fixed query answered, in the order the query
/// delivered them.
final class DeliveredFocuses: @unchecked Sendable {
    private let lock = NSLock()
    private var focuses: [BridgeCoverKey?] = []

    func record(_ focus: BridgeCoverKey?) {
        lock.withLock { focuses.append(focus) }
        Wait.happened()
    }

    /// How many values the query has delivered.
    var count: Int {
        lock.withLock { focuses.count }
    }

    /// Whether the query has delivered a value that focuses `key`.
    func delivered(_ key: BridgeCoverKey) -> Bool {
        lock.withLock { focuses.contains(key) }
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
