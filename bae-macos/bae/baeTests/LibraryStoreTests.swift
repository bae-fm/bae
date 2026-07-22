import BaeKit
import Foundation
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
    cover: BridgeImageRef? = nil
) -> BridgeAlbum {
    BridgeAlbum(
        id: id,
        title: title,
        year: year,
        isCompilation: false,
        artistNames: artistNames,
        releaseIds: releaseIds ?? [primaryReleaseId],
        primaryReleaseId: primaryReleaseId,
        cover: cover
    )
}

private func makeBridgeRelease(
    id: String = "release-1",
    albumId: String = "album-1",
    displayName: String = "Release One",
    format: String? = "FLAC",
    storageState: BridgeReleaseStorageState = .remote,
    pinned: Bool = false,
    storageActions: [BridgeReleaseStorageAction] = [],
    totalDuration: BridgeDurationUnits? = .hoursAndMinutes(
        hours: 1,
        minutes: 45
    ),
    fileCount: Int64 = 0,
    totalSize: Int64 = 0,
    cover: BridgeImageRef? = nil
) -> BridgeRelease {
    BridgeRelease(
        id: id,
        albumId: albumId,
        displayName: displayName,
        year: 2024,
        format: format,
        label: nil,
        catalogNumber: nil,
        country: nil,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: nil,
        tracks: [],
        trackGroups: [],
        files: [],
        imageFiles: [],
        galleryItems: [],
        totalDuration: totalDuration,
        fileCount: fileCount,
        totalSize: totalSize,
        cover: cover
    )
}

private func makeBridgeReleaseSummary(
    id: String = "release-1",
    albumId: String = "album-1",
    format: String? = "FLAC",
    storageState: BridgeReleaseStorageState = .remote,
    pinned: Bool = false,
    storageActions: [BridgeReleaseStorageAction] = [],
    fileCount: Int64 = 0,
    totalSize: Int64 = 0,
    cover: BridgeImageRef? = nil
) -> BridgeReleaseSummary {
    BridgeReleaseSummary(
        id: id,
        albumId: albumId,
        format: format,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: nil,
        fileCount: fileCount,
        totalSize: totalSize,
        cover: cover
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
        },
        onError: { _ in },
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
        return Array(albums[start..<end])
    }
}

private struct PaginatedListTestError: LocalizedError, Sendable {
    let message: String

    var errorDescription: String? { message }
}

private final class ThrowingAlbumPageSource: PageSource, @unchecked Sendable {
    var albums: [BridgeAlbum]
    var countError: PaginatedListTestError?
    var pageError: PaginatedListTestError?

    init(
        albums: [BridgeAlbum],
        countError: PaginatedListTestError? = nil,
        pageError: PaginatedListTestError? = nil
    ) {
        self.albums = albums
        self.countError = countError
        self.pageError = pageError
    }

    func count() async throws -> Int {
        if let countError {
            throw countError
        }
        return albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        if let pageError {
            throw pageError
        }
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
    }
}

@Suite("SearchResults")
struct SearchResultsTests {

    @Test("work results carry linked release count")
    func workResultsCarryLinkedReleaseCount() {
        let results = SearchResults(
            bridge: BridgeSearchResults(
                albums: [],
                artists: [],
                tracks: [],
                composers: [],
                works: [
                    BridgeWorkSummary(
                        workId: "work-child-a",
                        title: "Work Title A",
                        disambiguation: nil,
                        workType: "part",
                        parentWorkId: "work-parent-a",
                        composerNames: "Composer Name A",
                        linkedReleaseCount: 1,
                        representativeReleaseId: "release-a",
                        representativeCover: nil
                    )
                ]
            ),
            query: "work"
        )

        #expect(results.works.first?.linkedReleaseCount == 1)
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
    @Test("a changed cover version updates cover on the same instance")
    func internUpdatesCover() {
        // A cover change re-interns the summary with a bumped content version.
        // The instance is identity-stable, so the cover card observing `cover`
        // re-renders and reloads.
        let store = LibraryStore()
        let first = store.internAlbumSummary(
            makeBridgeAlbum(
                cover: BridgeImageRef(
                    id: "cover-1",
                    version: "1000",
                    imageType: .cover
                )
            )
        )
        #expect(
            first.cover
                == BridgeImageRef(
                    id: "cover-1",
                    version: "1000",
                    imageType: .cover
                )
        )

        let second = store.internAlbumSummary(
            makeBridgeAlbum(
                cover: BridgeImageRef(
                    id: "cover-1",
                    version: "2000",
                    imageType: .cover
                )
            )
        )

        #expect(first === second)
        #expect(
            first.cover
                == BridgeImageRef(
                    id: "cover-1",
                    version: "2000",
                    imageType: .cover
                )
        )
    }
}

@Suite("LibraryStore.albumTotal")
struct AlbumTotalTests {
    @MainActor
    @Test("nil before any count is recorded, then tracks recorded values")
    func recordsAlbumTotal() {
        let store = LibraryStore()
        #expect(store.albumTotal == nil)
        store.setAlbumTotal(3)
        #expect(store.albumTotal == 3)
        store.setAlbumTotal(0)
        #expect(store.albumTotal == 0)
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
            storageState: .remote,
            pinned: false,
            totalSize: 100
        )
        let first = store.internReleaseSummary(bridge1)

        let bridge2 = makeBridgeReleaseSummary(
            storageState: .remote,
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
            storageState: .remote,
            pinned: true,
            fileCount: 12,
            totalSize: 5_000_000,
            cover: BridgeImageRef(
                id: "release-1",
                version: "7",
                imageType: .cover
            )
        )

        let summary = store.internReleaseSummary(bridge)

        #expect(summary.id == "release-1")
        #expect(summary.albumId == "album-1")
        #expect(summary.format == "MP3")
        #expect(summary.storageState == .remote)
        #expect(summary.pinned)
        #expect(summary.fileCount == 12)
        #expect(summary.totalSize == 5_000_000)
        #expect(
            summary.cover
                == BridgeImageRef(
                    id: "release-1",
                    version: "7",
                    imageType: .cover
                )
        )
    }

    @MainActor
    @Test("intern carries and updates the release's own cover")
    func internCarriesReleaseCover() {
        let store = LibraryStore()
        let first = store.internReleaseSummary(
            makeBridgeReleaseSummary(
                cover: BridgeImageRef(
                    id: "release-1",
                    version: "1",
                    imageType: .cover
                )
            )
        )
        #expect(
            first.cover
                == BridgeImageRef(
                    id: "release-1",
                    version: "1",
                    imageType: .cover
                )
        )

        // Re-interning with a bumped cover version updates the existing
        // instance in place rather than replacing it.
        let second = store.internReleaseSummary(
            makeBridgeReleaseSummary(
                cover: BridgeImageRef(
                    id: "release-1",
                    version: "2",
                    imageType: .cover
                )
            )
        )
        #expect(first === second)
        #expect(
            first.cover
                == BridgeImageRef(
                    id: "release-1",
                    version: "2",
                    imageType: .cover
                )
        )
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
    @Test(
        "detail wraps the identity-stable summary from releaseSummaries slice"
    )
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
            storageState: .remote,
            pinned: false
        )
        _ = store.internReleaseDetail(bridge1)
        let originalSummary = store.releaseSummaries["release-1"]!

        let bridge2 = makeBridgeRelease(
            displayName: "V2",
            storageState: .remote,
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
        #expect(detail.totalDuration == .hoursAndMinutes(hours: 1, minutes: 45))
        // Interning a detail also interns its wrapped summary; the slim fields
        // (here `format`) carry through from the same `BridgeRelease`.
        #expect(detail.summary.format == "FLAC")
    }

    @MainActor
    @Test("internAlbumDetail populates normalized slices")
    func internAlbumDetailPopulatesSlices() {
        let store = LibraryStore()
        let release1 = makeBridgeRelease(id: "r-1")
        let release2 = makeBridgeRelease(id: "r-2", displayName: "Release Two")
        let detail = makeBridgeAlbumDetail(
            releases: [release1, release2]
        )

        store.internAlbumDetail(detail)

        #expect(store.albumSummaries["album-1"] != nil)
        #expect(store.releaseSummaries["r-1"] != nil)
        #expect(store.releaseSummaries["r-2"] != nil)
        #expect(store.releaseDetails["r-1"] != nil)
        #expect(store.releaseDetails["r-2"] != nil)
        #expect(store.releaseDetails["r-2"]?.displayName == "Release Two")
    }

    @MainActor
    @Test("release detail nil snapshot removes release slices")
    func nilReleaseDetailSnapshotRemovesReleaseSlices() {
        let store = LibraryStore()
        store.internReleaseDetail(makeBridgeRelease())
        #expect(store.releaseSummaries["release-1"] != nil)
        #expect(store.releaseDetails["release-1"] != nil)

        store.applyReleaseDetailSnapshot(releaseId: "release-1", bridge: nil)

        #expect(store.releaseSummaries["release-1"] == nil)
        #expect(store.releaseDetails["release-1"] == nil)
    }
}

// MARK: - loadReleaseDetail failure surfacing

/// `findReleaseDetail` stub that throws for its first `failFirst` calls, then
/// returns `release`. Lets a test drive a failure and then a retry through the
/// real store method. `@unchecked Sendable` with a lock because the store runs
/// the closure on a detached task.
private final class DetailLoadProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var calls = 0
    private let failFirst: Int
    private let release: BridgeRelease?

    init(failFirst: Int, release: BridgeRelease?) {
        self.failFirst = failFirst
        self.release = release
    }

    func next() throws -> BridgeRelease? {
        lock.lock()
        defer { lock.unlock() }
        calls += 1
        if calls <= failFirst {
            throw PaginatedListTestError(message: "detail load failed")
        }
        return release
    }
}

@Suite("LibraryStore.loadReleaseDetail")
struct LoadReleaseDetailTests {

    @MainActor
    @Test(
        "a thrown load failure surfaces as a per-release error, not a swallow"
    )
    func failureSurfacesError() async {
        let store = LibraryStore()
        let library = Library(findReleaseDetail: { _ in
            throw PaginatedListTestError(message: "detail load failed")
        })

        await store.loadReleaseDetail(releaseId: "release-1", library: library)

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(
            store.releaseDetailErrors["release-1"]
                == DisplayError(line: "detail load failed")
        )
    }

    @MainActor
    @Test("a nil result surfaces a not-found error rather than spinning")
    func nilResultSurfacesNotFound() async {
        let store = LibraryStore()
        let library = Library(findReleaseDetail: { _ in nil })

        await store.loadReleaseDetail(releaseId: "release-1", library: library)

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.releaseDetailErrors["release-1"] != nil)
    }

    @MainActor
    @Test("retry after a failure clears the error and re-queries into content")
    func retryClearsErrorAndLoads() async {
        let probe = DetailLoadProbe(failFirst: 1, release: makeBridgeRelease())
        let store = LibraryStore()
        let library = Library(findReleaseDetail: { _ in try probe.next() })

        await store.loadReleaseDetail(releaseId: "release-1", library: library)
        #expect(store.releaseDetailErrors["release-1"] != nil)
        #expect(store.releaseDetails["release-1"] == nil)

        await store.loadReleaseDetail(releaseId: "release-1", library: library)
        #expect(store.releaseDetailErrors["release-1"] == nil)
        #expect(store.releaseDetails["release-1"] != nil)
    }

    @MainActor
    @Test("a successful load leaves no error")
    func successLeavesNoError() async {
        let store = LibraryStore()
        let library = Library(findReleaseDetail: { _ in
            makeBridgeRelease()
        })

        await store.loadReleaseDetail(releaseId: "release-1", library: library)

        #expect(store.releaseDetails["release-1"] != nil)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }
}

// MARK: - PaginatedList tests

@Suite("PaginatedList")
struct PaginatedListTests {

    @MainActor
    @Test("loadInitial sets totalCount, no positions loaded until loadRange")
    func loadInitialAllocates() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: [
                makeBridgeAlbum(id: "a1"),
                makeBridgeAlbum(id: "a2"),
                makeBridgeAlbum(id: "a3"),
            ]
        )

        await list.loadInitial()

        #expect(list.totalCount == 3)
        #expect(list.idAt(0) == nil)
        #expect(list.idAt(2) == nil)
    }

    @MainActor
    @Test("loadRange populates ids at correct positions and interns via ingest")
    func loadRangePopulatesPositions() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: [
                makeBridgeAlbum(id: "a1", title: "Alpha"),
                makeBridgeAlbum(id: "a2", title: "Beta"),
                makeBridgeAlbum(id: "a3", title: "Gamma"),
            ]
        )
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
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
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
        let albums = (0..<7)
            .map {
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
    @Test("loadInitial surfaces a cold count failure as initialLoadError")
    func loadInitialSurfacesInitialLoadError() async {
        // A cold count-load failure is not an empty library: it lands on the
        // list's `initialLoadError` (which the grid renders as error + Retry),
        // not on `onError` (which would surface a redundant banner over the
        // empty grid).
        let source = ThrowingAlbumPageSource(
            albums: [],
            countError: PaginatedListTestError(message: "count failed")
        )
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )

        await list.loadInitial()

        #expect(list.initialLoadError == DisplayError(line: "count failed"))
        #expect(errors.isEmpty)
        #expect(list.totalCount == 0)
    }

    @MainActor
    @Test("a successful retry clears initialLoadError and sets the count")
    func retryClearsInitialLoadError() async {
        let source = ThrowingAlbumPageSource(
            albums: [makeBridgeAlbum(id: "a1")],
            countError: PaginatedListTestError(message: "count failed")
        )
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { _ in },
        )
        await list.loadInitial()
        #expect(list.initialLoadError != nil)

        source.countError = nil
        await list.loadInitial()

        #expect(list.initialLoadError == nil)
        #expect(list.totalCount == 1)
    }

    @MainActor
    @Test("loadRange reports page errors")
    func loadRangeReportsPageErrors() async {
        let source = ThrowingAlbumPageSource(
            albums: [makeBridgeAlbum(id: "a1")],
            pageError: PaginatedListTestError(message: "page failed")
        )
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )

        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 1)

        #expect(errors == [DisplayError(line: "page failed")])
    }

    @MainActor
    @Test("invalidate reports count errors")
    func invalidateReportsCountErrors() async {
        let source = ThrowingAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1")
        ])
        var errors: [DisplayError] = []
        let list = AlbumList(
            pageSource: source,
            ingest: { _ in },
            onError: { error in DisplayError(error).map { errors.append($0) } },
        )
        await list.loadInitial()
        source.countError = PaginatedListTestError(message: "reload failed")

        list.invalidate()
        await list.awaitReload()

        #expect(errors == [DisplayError(line: "reload failed")])
    }

    @MainActor
    @Test(
        "invalidate keeps old state visible, then atomically swaps in refreshed shape"
    )
    func invalidateRefetches() async {
        let store = LibraryStore()
        let list = makeList(
            store: store,
            albums: [
                makeBridgeAlbum(id: "a1", title: "Alpha"),
                makeBridgeAlbum(id: "a2", title: "Beta"),
                makeBridgeAlbum(id: "a3", title: "Gamma"),
            ]
        )
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
    @Test(
        "invalidate re-fetches previously loaded ranges and surfaces new rows"
    )
    func invalidateSurfacesNewRows() async {
        let store = LibraryStore()
        // Start with one album; load it.
        let source = MutablePageSource(albums: [
            makeBridgeAlbum(id: "a1", title: "Alpha")
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
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
        return Array(albums[start..<end])
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

// MARK: - Segment coalescing

/// `insertSegment` is private, so these drive it through the real `loadRange`
/// path — the only way segments enter the list — and observe the merged result
/// through `allLoadedIds` / `idAt`. Coalescing and stale-generation discard are
/// visible there: without them the loaded ids would carry duplicates, gaps, or
/// stale positions.
@Suite("PaginatedList segment management")
struct PaginatedListSegmentTests {
    @MainActor
    private func fiveAlbumList(_ store: LibraryStore) -> AlbumList {
        makeList(
            store: store,
            albums: (0..<5).map { makeBridgeAlbum(id: "a\($0)") }
        )
    }

    @MainActor
    @Test("overlapping same-generation ranges merge without duplicates")
    func overlappingMerge() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 3)  // [0, 3)
        await list.loadRange(offset: 1, limit: 3)  // [1, 4) overlaps

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
    }

    @MainActor
    @Test("adjacent same-generation ranges coalesce into one contiguous run")
    func adjacentCoalesce() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 0, limit: 2)  // [0, 2)
        await list.loadRange(offset: 2, limit: 2)  // [2, 4)

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
        #expect(list.idAt(3) == "a3")
    }

    @MainActor
    @Test("a new range absorbs a same-generation segment it fully contains")
    func absorbsContainedSegment() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 1, limit: 2)  // [1, 3)
        await list.loadRange(offset: 0, limit: 4)  // [0, 4) contains it

        #expect(list.allLoadedIds == ["a0", "a1", "a2", "a3"])
    }

    @MainActor
    @Test("disjoint ranges stay separate and sort by position")
    func disjointSortsByPosition() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()

        await list.loadRange(offset: 3, limit: 2)  // [3, 5) loaded first
        await list.loadRange(offset: 0, limit: 2)  // [0, 2)

        #expect(list.allLoadedIds == ["a0", "a1", "a3", "a4"])
        #expect(list.idAt(2) == nil)  // the gap stays unloaded
    }

    @MainActor
    @Test("a fresh-generation range discards the stale segment it overlaps")
    func staleGenerationDiscard() async {
        let list = fiveAlbumList(LibraryStore())
        await list.loadInitial()
        await list.loadRange(offset: 0, limit: 4)  // generation 0: [0, 4)

        list.invalidate()
        await list.awaitReload()  // generation 1

        await list.loadRange(offset: 0, limit: 2)  // generation 1: [0, 2)

        // The stale generation-0 [0, 4) is dropped wholesale, not kept for its
        // non-overlapping tail — positions 2 and 3 revert to unloaded.
        #expect(list.allLoadedIds == ["a0", "a1"])
        #expect(list.idAt(2) == nil)
    }

    @MainActor
    @Test("concurrent loadRange for the same range issues a single fetch")
    func concurrentLoadRangeCoalesces() async {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1"),
            makeBridgeAlbum(id: "a2"),
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        async let first: Void = list.loadRange(offset: 0, limit: 2)
        // The first fetch's page() is now running and blocked on the gate, so
        // loadRange has already registered its in-flight task. The second caller
        // dedupes onto it instead of issuing its own query.
        await source.waitForPageEntry()

        async let second: Void = list.loadRange(offset: 0, limit: 2)
        await Task.yield()
        await source.openGate()

        _ = await (first, second)

        #expect(source.pageCallCount == 1)
        #expect(list.idAt(0) == "a1")
    }

    @MainActor
    @Test("a fetch whose generation is bumped mid-flight drops its result")
    func generationDropMidFetch() async {
        let store = LibraryStore()
        let source = GatedAlbumPageSource(albums: [
            makeBridgeAlbum(id: "a1"),
            makeBridgeAlbum(id: "a2"),
        ])
        let list = AlbumList(
            pageSource: source,
            ingest: { rows in
                for row in rows { _ = store.internAlbumSummary(row) }
            },
            onError: { _ in },
        )
        await list.loadInitial()

        async let load: Void = list.loadRange(offset: 0, limit: 2)
        await source.waitForPageEntry()  // fetch in flight at generation 0

        list.invalidate()
        await list.awaitReload()  // generation bumps to 1

        await source.openGate()
        await load

        // fetchRange sees the generation moved past its own and drops the rows —
        // no segment is inserted.
        #expect(list.idAt(0) == nil)
        #expect(list.allLoadedIds.isEmpty)
    }
}

/// One-shot async gate: `page()` awaits `wait()`; the test resumes every waiter
/// with `open()`. Lets a test hold a fetch mid-flight while it drives the list.
private actor FetchGate {
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var isOpen = false

    func wait() async {
        if isOpen { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func open() {
        isOpen = true
        for waiter in waiters { waiter.resume() }
        waiters.removeAll()
    }
}

/// Page source that blocks each `page()` on a gate the test opens, and signals
/// when a fetch enters. Used to exercise the in-flight dedupe and the
/// generation-drop guard, both of which need a fetch held mid-flight.
final class GatedAlbumPageSource: PageSource, @unchecked Sendable {
    let albums: [BridgeAlbum]
    var pageCallCount = 0

    private let gate = FetchGate()
    private let entries: AsyncStream<Void>
    private let entryContinuation: AsyncStream<Void>.Continuation

    init(albums: [BridgeAlbum]) {
        self.albums = albums
        (entries, entryContinuation) = AsyncStream.makeStream(of: Void.self)
    }

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        pageCallCount += 1
        entryContinuation.yield(())
        await gate.wait()
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
    }

    /// Suspend until a `page()` call has entered (and is blocked on the gate).
    func waitForPageEntry() async {
        var iterator = entries.makeAsyncIterator()
        _ = await iterator.next()
    }

    func openGate() async {
        await gate.open()
    }
}
