import Testing

@testable import bae

// MARK: - Test helpers

private func makeBridgeAlbum(
    id: String = "album-1",
    title: String = "Album Title",
    year: Int32? = 2024,
    artistNames: String = "Artist Name",
    releaseIds: [String]? = nil,
    primaryReleaseId: String = "release-1",
    coverPath: String? = nil
) -> BridgeAlbum {
    BridgeAlbum(
        id: id,
        title: title,
        year: year,
        isCompilation: false,
        artistNames: artistNames,
        releaseIds: releaseIds ?? [primaryReleaseId],
        primaryReleaseId: primaryReleaseId,
        coverPath: coverPath
    )
}

private func makeBridgeRelease(
    id: String = "release-1",
    albumId: String = "album-1",
    displayName: String = "Release One",
    format: String? = "FLAC",
    storageState: BridgeReleaseStorageState = .managed,
    pinned: Bool = false,
    storageActions: [BridgeReleaseStorageAction] = [],
    totalDurationMs: Int64 = 2_700_000,
    fileCount: Int64 = 0,
    totalSize: Int64 = 0,
    coverPath: String? = nil
) -> BridgeRelease {
    BridgeRelease(
        id: id,
        albumId: albumId,
        displayName: displayName,
        releaseName: nil,
        year: 2024,
        format: format,
        label: nil,
        catalogNumber: nil,
        country: nil,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        tracks: [],
        trackGroups: [],
        files: [],
        imageFiles: [],
        galleryItems: [],
        totalDurationMs: totalDurationMs,
        fileCount: fileCount,
        totalSize: totalSize,
        coverPath: coverPath
    )
}

private func makeBridgeReleaseSummary(
    id: String = "release-1",
    albumId: String = "album-1",
    format: String? = "FLAC",
    storageState: BridgeReleaseStorageState = .managed,
    pinned: Bool = false,
    storageActions: [BridgeReleaseStorageAction] = [],
    fileCount: Int64 = 0,
    totalSize: Int64 = 0,
    coverPath: String? = nil
) -> BridgeReleaseSummary {
    BridgeReleaseSummary(
        id: id,
        albumId: albumId,
        format: format,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        fileCount: fileCount,
        totalSize: totalSize,
        coverPath: coverPath
    )
}

private func makeBridgeAlbumDetail(
    albumId: String = "album-1",
    title: String = "Album Title",
    releases: [BridgeRelease] = []
) -> BridgeAlbumDetail {
    BridgeAlbumDetail(
        album: makeBridgeAlbum(
            id: albumId,
            title: title,
            releaseIds: releases.map(\.id),
        ),
        releases: releases,
    )
}

@MainActor
private func makeList(store: LibraryStore, albums: [BridgeAlbum]) -> AlbumList {
    AlbumList(
        pageSource: AlbumPreviewPageSource(albums: albums),
        ingest: { rows in
            for row in rows {
                _ = store.internAlbumSummary(row)
            }
        }
    )
}

/// Test-only page source that counts `page()` invocations. Used to pin
/// the `loadRange` fast-path guard — interning alone is identity-stable,
/// so a naive idempotency assertion passes whether or not `page()` was
/// actually called a second time.
final class CountingAlbumPageSource: PageSource, @unchecked Sendable {
    let albums: [BridgeAlbum]
    var pageCallCount = 0

    init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        pageCallCount += 1
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start ..< end])
    }
}

// MARK: - internAlbumSummary tests

@Suite("LibraryStore.internAlbumSummary")
struct InternAlbumSummaryTests {

    @MainActor
    @Test("intern twice returns same identity")
    func internTwiceSameIdentity() {
        let store = LibraryStore()
        let bridge = makeBridgeAlbum()

        let first = store.internAlbumSummary(bridge)
        let second = store.internAlbumSummary(bridge)

        #expect(first === second)
        #expect(store.albumSummaries.count == 1)
    }

    @MainActor
    @Test("intern with updated fields preserves identity, updates fields")
    func internUpdatesFields() {
        let store = LibraryStore()
        let bridge1 = makeBridgeAlbum(title: "Old Title")
        let first = store.internAlbumSummary(bridge1)

        let bridge2 = makeBridgeAlbum(title: "New Title")
        let second = store.internAlbumSummary(bridge2)

        #expect(first === second)
        #expect(first.title == "New Title")
    }

    @MainActor
    @Test("intern carries releaseIds from bridge")
    func internCarriesReleaseIds() {
        let store = LibraryStore()
        let bridge = makeBridgeAlbum(releaseIds: ["r-1", "r-2", "r-3"])

        let summary = store.internAlbumSummary(bridge)

        #expect(summary.releaseIds == ["r-1", "r-2", "r-3"])
    }

    @MainActor
    @Test("a changed cover identifier updates coverPath on the same instance")
    func internUpdatesCoverPath() {
        // A cover change re-interns the summary with a new cache-busting
        // identifier (`<path>#v=<mtime>`). The instance is identity-stable, so
        // the cover card observing `coverPath` re-renders and reloads.
        let store = LibraryStore()
        let first = store.internAlbumSummary(
            makeBridgeAlbum(coverPath: "/covers/release-1#v=1000"))
        #expect(first.coverPath == "/covers/release-1#v=1000")

        let second = store.internAlbumSummary(
            makeBridgeAlbum(coverPath: "/covers/release-1#v=2000"))

        #expect(first === second)
        #expect(first.coverPath == "/covers/release-1#v=2000")
    }
}

// MARK: - internReleaseSummary tests

@Suite("LibraryStore.internReleaseSummary")
struct InternReleaseSummaryTests {

    @MainActor
    @Test("intern twice returns same identity")
    func internTwiceSameIdentity() {
        let store = LibraryStore()
        let bridge = makeBridgeReleaseSummary()

        let first = store.internReleaseSummary(bridge)
        let second = store.internReleaseSummary(bridge)

        #expect(first === second)
        #expect(store.releaseSummaries.count == 1)
    }

    @MainActor
    @Test("intern with updated fields preserves identity, updates fields")
    func internUpdatesFields() {
        let store = LibraryStore()
        let bridge1 = makeBridgeReleaseSummary(
            storageState: .managed,
            pinned: false,
            totalSize: 100
        )
        let first = store.internReleaseSummary(bridge1)

        let bridge2 = makeBridgeReleaseSummary(
            storageState: .managed,
            pinned: true,
            totalSize: 200
        )
        let second = store.internReleaseSummary(bridge2)

        #expect(first === second)
        #expect(first.pinned)
        #expect(first.totalSize == 200)
    }

    @MainActor
    @Test("intern from BridgeRelease populates summary fields")
    func internFromBridgeRelease() {
        let store = LibraryStore()
        let bridge = makeBridgeRelease(
            format: "MP3",
            storageState: .managed,
            pinned: true,
            fileCount: 12,
            totalSize: 5_000_000,
            coverPath: "/img/release-1.jpg#v=7"
        )

        let summary = store.internReleaseSummary(bridge)

        #expect(summary.id == "release-1")
        #expect(summary.albumId == "album-1")
        #expect(summary.format == "MP3")
        #expect(summary.storageState == .managed)
        #expect(summary.pinned)
        #expect(summary.fileCount == 12)
        #expect(summary.totalSize == 5_000_000)
        #expect(summary.coverPath == "/img/release-1.jpg#v=7")
    }

    @MainActor
    @Test("intern carries and updates the release's own cover")
    func internCarriesReleaseCover() {
        let store = LibraryStore()
        let first = store.internReleaseSummary(
            makeBridgeReleaseSummary(coverPath: "/img/release-1.jpg#v=1")
        )
        #expect(first.coverPath == "/img/release-1.jpg#v=1")

        // Re-interning with a bumped cover identifier updates the existing
        // instance in place rather than replacing it.
        let second = store.internReleaseSummary(
            makeBridgeReleaseSummary(coverPath: "/img/release-1.jpg#v=2")
        )
        #expect(first === second)
        #expect(first.coverPath == "/img/release-1.jpg#v=2")
    }
}

// MARK: - InternReleaseDetail tests

@Suite("LibraryStore.internReleaseDetail")
struct InternReleaseDetailTests {

    @MainActor
    @Test("intern populates both releaseDetails and releaseSummaries slices")
    func internPopulatesBothSlices() {
        let store = LibraryStore()
        let bridge = makeBridgeRelease()

        _ = store.internReleaseDetail(bridge)

        #expect(store.releaseDetails["release-1"] != nil)
        #expect(store.releaseSummaries["release-1"] != nil)
    }

    @MainActor
    @Test("detail wraps the identity-stable summary from releaseSummaries slice")
    func detailWrapsCanonicalSummary() {
        let store = LibraryStore()
        let bridge = makeBridgeRelease()

        _ = store.internReleaseDetail(bridge)

        let summaryFromDetail = store.releaseDetails["release-1"]!.summary
        let summaryFromSlice = store.releaseSummaries["release-1"]!

        #expect(summaryFromDetail === summaryFromSlice)
    }

    @MainActor
    @Test("intern twice preserves summary identity, replaces detail wholesale")
    func internTwicePreservesSummaryIdentity() {
        let store = LibraryStore()
        let bridge1 = makeBridgeRelease(
            displayName: "V1",
            storageState: .managed,
            pinned: false
        )
        _ = store.internReleaseDetail(bridge1)
        let originalSummary = store.releaseSummaries["release-1"]!

        let bridge2 = makeBridgeRelease(
            displayName: "V2",
            storageState: .managed,
            pinned: true
        )
        _ = store.internReleaseDetail(bridge2)

        #expect(store.releaseSummaries["release-1"] === originalSummary)
        #expect(originalSummary.pinned)
        #expect(store.releaseDetails["release-1"]!.displayName == "V2")
        // Detail's summary pointer still matches the canonical one.
        #expect(store.releaseDetails["release-1"]!.summary === originalSummary)
    }

    @MainActor
    @Test("detail carries fat fields from BridgeRelease")
    func detailCarriesFatFields() {
        let store = LibraryStore()
        let bridge = makeBridgeRelease(displayName: "Deluxe Edition")

        _ = store.internReleaseDetail(bridge)

        let detail = store.releaseDetails["release-1"]!
        #expect(detail.displayName == "Deluxe Edition")
        #expect(detail.totalDurationMs == 2_700_000)
        // Interning a detail also interns its wrapped summary; the slim fields
        // (here `format`) carry through from the same `BridgeRelease`.
        #expect(detail.summary.format == "FLAC")
    }
}

// MARK: - Album event tests

@Suite("LibraryStore album events")
struct AlbumEventTests {

    @MainActor
    @Test("handleAlbumAdded populates normalized slices")
    func albumAddedPopulatesSlices() {
        let store = LibraryStore()
        let release1 = makeBridgeRelease(id: "r-1")
        let release2 = makeBridgeRelease(id: "r-2", displayName: "Release Two")
        let detail = makeBridgeAlbumDetail(
            releases: [release1, release2]
        )

        store.handleAlbumAdded(album: detail)

        #expect(store.albumSummaries["album-1"] != nil)
        #expect(store.releaseSummaries["r-1"] != nil)
        #expect(store.releaseSummaries["r-2"] != nil)
        #expect(store.releaseDetails["r-1"] != nil)
        #expect(store.releaseDetails["r-2"] != nil)
        #expect(store.releaseDetails["r-2"]?.displayName == "Release Two")
    }

    @MainActor
    @Test("handleAlbumAdded does not mutate live lists")
    func albumAddedDoesNotMutateList() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [makeBridgeAlbum(id: "a1", title: "Existing")])
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 1)

        let preCount = list.totalCount

        store.handleAlbumAdded(album: makeBridgeAlbumDetail(albumId: "a2", title: "New"))

        // Slice holds the new summary, but the list's segments are untouched.
        #expect(store.albumSummaries["a2"] != nil)
        #expect(list.totalCount == preCount)
        #expect(list.idAt(0) == "a1")
    }

    @MainActor
    @Test("handleAlbumUpdated updates existing summary, same identity")
    func albumUpdated() {
        let store = LibraryStore()
        let detail1 = makeBridgeAlbumDetail(title: "Old")
        store.handleAlbumAdded(album: detail1)
        let original = store.albumSummaries["album-1"]!

        let detail2 = makeBridgeAlbumDetail(title: "New")
        store.handleAlbumUpdated(album: detail2)

        #expect(store.albumSummaries["album-1"] === original)
        #expect(original.title == "New")
    }

    @MainActor
    @Test("handleAlbumUpdated with no prior album interns the payload")
    func albumUpdatedWithoutPriorSummary() {
        let store = LibraryStore()
        let r1 = makeBridgeRelease(id: "r-1")
        let detail = makeBridgeAlbumDetail(
            releases: [r1]
        )

        // Simulates re-subscribe / out-of-order delivery: the reducer
        // receives `AlbumUpdated` with no prior `AlbumAdded` state.
        // The reducer must not drop the payload.
        store.handleAlbumUpdated(album: detail)

        #expect(store.albumSummaries["album-1"]?.title == "Album Title")
        #expect(store.releaseSummaries["r-1"] != nil)
        #expect(store.releaseDetails["r-1"] != nil)
    }

    @MainActor
    @Test("handleAlbumUpdated replaces releaseDetails and updates release summary identity")
    func albumUpdatedRefreshesSlices() {
        let store = LibraryStore()
        let r1 = makeBridgeRelease(
            id: "r-1",
            displayName: "V1",
            storageState: .managed,
            pinned: false
        )
        let initialDetail = makeBridgeAlbumDetail(
            releases: [r1]
        )
        store.handleAlbumAdded(album: initialDetail)
        let originalSummary = store.releaseSummaries["r-1"]!

        let updatedR1 = makeBridgeRelease(
            id: "r-1",
            displayName: "V2",
            storageState: .managed,
            pinned: true
        )
        let updatedDetail = makeBridgeAlbumDetail(
            releases: [updatedR1]
        )
        store.handleAlbumUpdated(album: updatedDetail)

        // Summary identity preserved; fields updated in place.
        #expect(store.releaseSummaries["r-1"] === originalSummary)
        #expect(originalSummary.pinned)

        // Detail replaced wholesale; still wraps canonical summary.
        #expect(store.releaseDetails["r-1"]!.displayName == "V2")
        #expect(store.releaseDetails["r-1"]!.summary === originalSummary)
    }

    @MainActor
    @Test("handleAlbumRemoved removes summary and all its releases from slices")
    func albumRemoved() {
        let store = LibraryStore()
        let r1 = makeBridgeRelease(id: "r-1", albumId: "album-1")
        let r2 = makeBridgeRelease(id: "r-2", albumId: "album-1")
        store.handleAlbumAdded(album: makeBridgeAlbumDetail(releases: [r1, r2]))

        store.handleAlbumRemoved(albumId: "album-1", releaseIds: ["r-1", "r-2"])

        #expect(store.albumSummaries["album-1"] == nil)
        #expect(store.releaseSummaries["r-1"] == nil)
        #expect(store.releaseSummaries["r-2"] == nil)
        #expect(store.releaseDetails["r-1"] == nil)
        #expect(store.releaseDetails["r-2"] == nil)
    }

    @MainActor
    @Test("handleAlbumRemoved cascades only the removed album's releases")
    func albumRemovedCascadeScoped() {
        let store = LibraryStore()
        let a1r1 = makeBridgeRelease(id: "a1-r1", albumId: "album-1")
        let a1r2 = makeBridgeRelease(id: "a1-r2", albumId: "album-1")
        let a2r1 = makeBridgeRelease(id: "a2-r1", albumId: "album-2")
        let a2r2 = makeBridgeRelease(id: "a2-r2", albumId: "album-2")

        store.handleAlbumAdded(album: makeBridgeAlbumDetail(
            albumId: "album-1",
            releases: [a1r1, a1r2]
        ))
        store.handleAlbumAdded(album: makeBridgeAlbumDetail(
            albumId: "album-2",
            releases: [a2r1, a2r2]
        ))

        store.handleAlbumRemoved(albumId: "album-1", releaseIds: ["a1-r1", "a1-r2"])

        // album-1 and both of its releases are gone.
        #expect(store.albumSummaries["album-1"] == nil)
        #expect(store.releaseSummaries["a1-r1"] == nil)
        #expect(store.releaseSummaries["a1-r2"] == nil)
        #expect(store.releaseDetails["a1-r1"] == nil)
        #expect(store.releaseDetails["a1-r2"] == nil)

        // album-2 and both of its releases survive.
        #expect(store.albumSummaries["album-2"] != nil)
        #expect(store.releaseSummaries["a2-r1"] != nil)
        #expect(store.releaseSummaries["a2-r2"] != nil)
        #expect(store.releaseDetails["a2-r1"] != nil)
        #expect(store.releaseDetails["a2-r2"] != nil)
    }

    @MainActor
    @Test("handleAlbumRemoved does not mutate live lists")
    func albumRemovedDoesNotMutateList() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
        ])
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 2)

        let preCount = list.totalCount

        store.handleAlbumRemoved(albumId: "a2", releaseIds: [])

        // Slice shrinks, but the list's segments are untouched until invalidate().
        #expect(store.albumSummaries["a2"] == nil)
        #expect(list.totalCount == preCount)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(1) == "a2")
    }
}

// MARK: - Release event tests

@Suite("LibraryStore release events")
struct ReleaseEventTests {

    @MainActor
    @Test("handleReleaseAdded interns the release and updates parent album releaseIds from event")
    func releaseAddedInternsSlices() {
        let store = LibraryStore()
        store.handleAlbumAdded(album: makeBridgeAlbumDetail(albumId: "album-1"))
        let newRelease = makeBridgeRelease(id: "release-2", displayName: "Second Release")
        let updatedAlbum = makeBridgeAlbum(
            id: "album-1",
            releaseIds: ["release-1", "release-2"]
        )

        store.handleReleaseAdded(album: updatedAlbum, release: newRelease)

        #expect(store.releaseSummaries["release-2"]?.id == "release-2")
        #expect(store.releaseDetails["release-2"]?.displayName == "Second Release")
        #expect(store.albumSummaries["album-1"]?.releaseIds.contains("release-2") == true)
    }

    @MainActor
    @Test("handleReleaseUpdated replaces detail, preserves summary identity")
    func releaseUpdated() {
        let store = LibraryStore()
        store.handleReleaseAdded(
            album: makeBridgeAlbum(id: "album-1", releaseIds: ["release-1"]),
            release: makeBridgeRelease(displayName: "Old")
        )
        let originalSummary = store.releaseSummaries["release-1"]!

        let updated = makeBridgeRelease(
            displayName: "New",
            storageState: .managed,
            pinned: true
        )
        store.handleReleaseUpdated(release: updated)

        #expect(store.releaseDetails["release-1"]?.displayName == "New")
        #expect(store.releaseSummaries["release-1"] === originalSummary)
        #expect(originalSummary.pinned)
    }

    @MainActor
    @Test("handleReleaseRemoved drops the release and interns the event's post-removal album")
    func releaseRemoved() {
        let store = LibraryStore()
        store.handleAlbumAdded(
            album: makeBridgeAlbumDetail(
                albumId: "album-1",
                releases: [
                    makeBridgeRelease(id: "release-1"),
                    makeBridgeRelease(id: "release-2"),
                ]
            )
        )
        store.albumSummaries["album-1"]?.releaseIds = ["release-1", "release-2"]
        #expect(store.releaseSummaries["release-1"] != nil)
        #expect(store.releaseDetails["release-1"] != nil)

        // The event carries the parent album's post-removal summary, exactly
        // as the core emitter ships it — releaseIds no longer lists release-1.
        store.handleReleaseRemoved(
            releaseId: "release-1",
            album: makeBridgeAlbum(id: "album-1", releaseIds: ["release-2"])
        )

        #expect(store.releaseSummaries["release-1"] == nil)
        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.albumSummaries["album-1"]?.releaseIds == ["release-2"])
    }

    @MainActor
    @Test("handleReleaseRemoved leaves the album untouched when the event carries no summary")
    func releaseRemovedWithoutAlbum() {
        let store = LibraryStore()
        store.handleAlbumAdded(
            album: makeBridgeAlbumDetail(albumId: "album-1", releases: [makeBridgeRelease()])
        )

        // album: nil mirrors the cascade-delete case where AlbumRemoved already
        // dropped the summary; the release slices still clear.
        store.handleReleaseRemoved(releaseId: "release-1", album: nil)

        #expect(store.releaseSummaries["release-1"] == nil)
        #expect(store.releaseDetails["release-1"] == nil)
    }
}

// MARK: - PaginatedList tests

@Suite("PaginatedList")
struct PaginatedListTests {

    @MainActor
    @Test("loadInitial sets totalCount, no positions loaded until loadRange")
    func loadInitialAllocates() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [
            makeBridgeAlbum(id: "a1"),
            makeBridgeAlbum(id: "a2"),
            makeBridgeAlbum(id: "a3"),
        ])

        await list.loadInitial()

        #expect(list.totalCount == 3)
        #expect(list.idAt(0) == nil)
        #expect(list.idAt(2) == nil)
    }

    @MainActor
    @Test("loadRange populates ids at correct positions and interns via ingest")
    func loadRangePopulatesPositions() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
            makeBridgeAlbum(id: "a3", title: "Gamma"),
        ])
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 3)

        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(1) == "a2")
        #expect(list.idAt(2) == "a3")
        #expect(store.albumSummaries["a1"]?.title == "Alpha")
        #expect(store.albumSummaries["a2"]?.title == "Beta")
        #expect(store.albumSummaries["a3"]?.title == "Gamma")
    }

    @MainActor
    @Test("loadRange is idempotent — skips fully-loaded ranges")
    func loadRangeIdempotent() async {
        let store = LibraryStore()
        let source = CountingAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
        ])
        let list = AlbumList(pageSource: source, ingest: { rows in
            for row in rows { _ = store.internAlbumSummary(row) }
        })
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 2)
        #expect(source.pageCallCount == 1)

        await list.loadRange(offset: 0, limit: 2)

        #expect(source.pageCallCount == 1)
    }

    @MainActor
    @Test("rowCount computes correctly")
    func rowCountComputation() async {
        let store = LibraryStore()
        let albums = (0..<7).map {
            makeBridgeAlbum(id: "a\($0)", title: "Album \($0)")
        }
        let list = makeList(store: store, albums: albums)
        await list.loadInitial()

        #expect(list.rowCount(columnCount: 4) == 2)
        #expect(list.rowCount(columnCount: 3) == 3)
        #expect(list.rowCount(columnCount: 1) == 7)
        #expect(list.rowCount(columnCount: 0) == 0)
    }

    @MainActor
    @Test("empty page source yields totalCount == 0 and empty ids")
    func emptyList() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [])

        await list.loadInitial()

        #expect(list.totalCount == 0)
        #expect(list.idAt(0) == nil)
    }

    @MainActor
    @Test("invalidate keeps old state visible, then atomically swaps in refreshed shape")
    func invalidateRefetches() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
            makeBridgeAlbum(id: "a3", title: "Gamma"),
        ])
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 3)
        #expect(list.idAt(0) == "a1")
        #expect(list.totalCount == 3)

        list.invalidate()

        // Synchronous state immediately after invalidate(): old totalCount
        // and stale segments remain visible while the count fetch runs.
        #expect(list.totalCount == 3)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(2) == "a3")

        // After reload: count confirmed, generation bumped, stale segments
        // stay in place until tasks re-fetch them.
        await list.awaitReload()

        #expect(list.totalCount == 3)
        #expect(list.generation == 1)
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(2) == "a3")
    }

    @MainActor
    @Test("invalidate re-fetches previously loaded ranges and surfaces new rows")
    func invalidateSurfacesNewRows() async {
        let store = LibraryStore()
        // Start with one album; load it.
        let source = MutablePageSource(albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
        ])
        let list = AlbumList(pageSource: source, ingest: { rows in
            for row in rows { _ = store.internAlbumSummary(row) }
        })
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 1)
        #expect(list.idAt(0) == "a1")

        // Simulate an import: the page source now returns two rows.
        source.albums = [
            makeBridgeAlbum(id: "a1", title: "Alpha"),
            makeBridgeAlbum(id: "a2", title: "Beta"),
        ]
        list.invalidate()
        await list.awaitReload()

        #expect(list.totalCount == 2)
        // Stale gen-0 segment keeps a1 visible at position 0. Position 1
        // is unloaded — the per-row `.task(id:)` in the consuming view drives
        // lazy fills once the view restarts its task for the new generation.
        #expect(list.idAt(0) == "a1")
        #expect(list.idAt(1) == nil)
    }
}

// MARK: - Mutable page source (for invalidate tests)

/// Test-only page source whose backing array can be mutated between
/// calls. Used to simulate a shape change (import, delete) that
/// `invalidate()` picks up on the next reload.
final class MutablePageSource: PageSource, @unchecked Sendable {
    var albums: [BridgeAlbum]

    init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start ..< end])
    }
}

// MARK: - RowLoadID (row task-restart identity)

/// Every paginated row keys its load `.task(id:)` on a `RowLoadID` (the list's
/// `loadEpoch` + the row position). These exercise that id — the value the views
/// actually key on — across the three transitions that decide whether a row
/// refetches: a swap to a fresh list instance, an invalidation, and a plain
/// content load. The id must change on the first two (or a swapped-in/invalidated
/// row stays stuck on its placeholder) and must NOT change on the third (or every
/// page fetch needlessly restarts every visible row).
@Suite("PaginatedList row load identity")
struct PaginatedListRowLoadIDTests {
    @MainActor
    @Test("a row's task id differs across a list swap, at a fixed position")
    func differsAcrossSwap() async {
        let store = LibraryStore()
        let albums = [makeBridgeAlbum(id: "a1")]
        let first = makeList(store: store, albums: albums)
        let second = makeList(store: store, albums: albums)
        await first.loadInitial()
        await second.loadInitial()

        // Both fresh lists sit at generation 0, so generation (or position)
        // alone can't tell them apart — the instance identity in the epoch is
        // what makes the swapped-in row's task restart.
        #expect(first.generation == second.generation)
        #expect(
            RowLoadID(epoch: first.loadEpoch, index: 0)
                != RowLoadID(epoch: second.loadEpoch, index: 0)
        )
    }

    @MainActor
    @Test("a row's task id changes when the list is invalidated")
    func changesOnInvalidate() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [makeBridgeAlbum(id: "a1")])
        await list.loadInitial()
        let before = RowLoadID(epoch: list.loadEpoch, index: 0)

        list.invalidate()
        await list.awaitReload()

        #expect(RowLoadID(epoch: list.loadEpoch, index: 0) != before)
    }

    @MainActor
    @Test("a content load leaves a row's task id unchanged")
    func stableAcrossContentLoad() async {
        let store = LibraryStore()
        let list = makeList(store: store, albums: [makeBridgeAlbum(id: "a1")])
        await list.loadInitial()
        let before = RowLoadID(epoch: list.loadEpoch, index: 0)

        await list.loadRange(offset: 0, limit: 1)

        #expect(RowLoadID(epoch: list.loadEpoch, index: 0) == before)
    }
}
