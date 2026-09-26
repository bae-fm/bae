import BaeKit
import Foundation
import Testing

@testable import bae

// MARK: - Test helpers

func makeBridgeAlbum(
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
    media: [BridgeMediaCount] = PreviewData.media(.cd),
    storageState: BridgeReleaseStorageState = .remote,
    pinned: Bool = false,
    storageActions: [BridgeReleaseStorageAction] = [],
    transferAction: BridgeReleaseStorageAction? = nil,
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
        name: .named(name: displayName),
        year: 2024,
        label: nil,
        catalogNumber: nil,
        facts: PreviewData.pressingFacts(media: media),
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: transferAction,
        tracks: [],
        trackGroups: [],
        files: [],
        sourceAudio: nil,
        imageFiles: [],
        coverFiles: [],
        galleryItems: [],
        records: [],
        totalDuration: totalDuration,
        fileCount: fileCount,
        totalSize: totalSize,
        cover: cover
    )
}

private func makeBridgeReleaseSummary(
    id: String = "release-1",
    albumId: String = "album-1",
    media: [BridgeMediaCount] = PreviewData.media(.digital),
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
        media: media,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: nil,
        fileCount: fileCount,
        totalSize: totalSize,
        cover: cover
    )
}

func makeBridgeAlbumDetail(
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
func makeList(store: LibraryStore, albums: [BridgeAlbum]) -> AlbumList {
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

final class TestPageSubscription: PageSubscription, @unchecked Sendable {
    private let task: Task<Void, Never>

    init(_ task: Task<Void, Never>) {
        self.task = task
    }

    func cancel() {
        task.cancel()
    }
}

/// Test-only page source that counts subscriptions. Used to pin
/// the `loadRange` fast-path guard — interning alone is identity-stable,
/// so a naive idempotency assertion passes whether or not another page query
/// was actually subscribed.
final class CountingAlbumPageSource: PageSource, @unchecked Sendable {
    let albums: [BridgeAlbum]
    var pageCallCount = 0

    init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
        onError _: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        pageCallCount += 1
        let albums = albums
        return TestPageSubscription(
            Task { @MainActor in
                let start = min(offset, albums.count)
                let end = min(start + limit, albums.count)
                onValue(Array(albums[start..<end]), albums.count)
            }
        )
    }
}

struct PaginatedListTestError: LocalizedError, Sendable {
    let message: String

    var errorDescription: String? { message }
}

final class ThrowingAlbumPageSource: PageSource, @unchecked Sendable {
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

    func subscribe(
        offset: Int,
        limit: Int,
        onValue: @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let albums = albums
        let error = offset == 0 && limit == 50 ? countError : pageError
        return TestPageSubscription(
            Task { @MainActor in
                if let error {
                    onError(error)
                    return
                }
                let start = min(offset, albums.count)
                let end = min(start + limit, albums.count)
                onValue(Array(albums[start..<end]), albums.count)
            }
        )
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
            media: PreviewData.media(.vinyl, 2),
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
        #expect(summary.media == PreviewData.media(.vinyl, 2))
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
    @Test("intern from BridgeRelease preserves an active storage transition")
    func internFromBridgeReleasePreservesTransfer() {
        let store = LibraryStore()

        let summary = store.internReleaseSummary(
            makeBridgeRelease(transferAction: .makeRemote)
        )

        #expect(summary.transfer != nil)
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
    func detailWrapsCanonicalSummary() throws {
        let store = LibraryStore()
        let bridge = makeBridgeRelease()

        _ = store.internReleaseDetail(bridge)

        let summaryFromDetail = try #require(
            store.releaseDetails["release-1"]
        )
        .summary
        let summaryFromSlice = try #require(
            store.releaseSummaries["release-1"]
        )

        #expect(summaryFromDetail === summaryFromSlice)
    }

    @MainActor
    @Test("intern twice preserves summary identity, replaces detail wholesale")
    func internTwicePreservesSummaryIdentity() throws {
        let store = LibraryStore()
        let bridge1 = makeBridgeRelease(
            displayName: "V1",
            storageState: .remote,
            pinned: false
        )
        _ = store.internReleaseDetail(bridge1)
        let originalSummary = try #require(
            store.releaseSummaries["release-1"]
        )

        let bridge2 = makeBridgeRelease(
            displayName: "V2",
            storageState: .remote,
            pinned: true
        )
        _ = store.internReleaseDetail(bridge2)

        #expect(store.releaseSummaries["release-1"] === originalSummary)
        #expect(originalSummary.pinned)
        let updatedDetail = try #require(store.releaseDetails["release-1"])
        #expect(updatedDetail.displayName == "V2")
        // Detail's summary pointer still matches the canonical one.
        #expect(updatedDetail.summary === originalSummary)
    }

    @MainActor
    @Test("detail carries fat fields from BridgeRelease")
    func detailCarriesFatFields() throws {
        let store = LibraryStore()
        let bridge = makeBridgeRelease(displayName: "Deluxe Edition")

        _ = store.internReleaseDetail(bridge)

        let detail = try #require(store.releaseDetails["release-1"])
        #expect(detail.displayName == "Deluxe Edition")
        #expect(detail.totalDuration == .hoursAndMinutes(hours: 1, minutes: 45))
        // Interning a detail also interns its wrapped summary; the slim fields
        // (here the release media) carry through from the same `BridgeRelease`.
        #expect(detail.summary.media == PreviewData.media(.cd))
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

// MARK: - Detail reads

/// A detail read the test drives: it records every id it is pointed at and
/// every read opened, and hands each read's `next` whatever the test emits
/// on it.
final class DetailFeed<Value: Sendable>: @unchecked Sendable {
    private final class Read: @unchecked Sendable {
        var pending: [Result<DetailDelivery<Value>, any Error>] = []
        var waiter: CheckedContinuation<DetailDelivery<Value>, any Error>?
        var cancelled = false
        var asks = 0
    }

    private let lock = NSLock()
    private var reads: [Read] = []
    private var ids: [String?] = []

    var opened: Int { lock.withLock { reads.count } }
    var requested: [String?] { lock.withLock { ids } }

    func isCancelled(read index: Int) -> Bool {
        lock.withLock { reads[index].cancelled }
    }

    /// How many times read `index` has been asked for a value. The reader
    /// takes one value at a time and applies it before asking again, so an
    /// ask past a delivered value means that value has been handled, applied
    /// or not.
    func asks(read index: Int = 0) -> Int {
        lock.withLock { reads[index].asks }
    }

    func query() -> DetailQuery<Value> {
        let read = Read()
        lock.withLock { reads.append(read) }
        return DetailQuery(
            setId: { [self] id in lock.withLock { ids.append(id) } },
            next: { [self] in
                try await withCheckedThrowingContinuation { continuation in
                    let ready: Result<DetailDelivery<Value>, any Error>? =
                        lock.withLock {
                            read.asks += 1
                            if read.pending.isEmpty {
                                read.waiter = continuation
                                return nil
                            }
                            return read.pending.removeFirst()
                        }
                    if let ready { continuation.resume(with: ready) }
                }
            },
            cancel: { [self] in
                let waiter = lock.withLock {
                    read.cancelled = true
                    let waiter = read.waiter
                    read.waiter = nil
                    return waiter
                }
                waiter?.resume(throwing: BridgeError.Cancelled)
            }
        )
    }

    func emit(read index: Int = 0, id: String, value: Value?) {
        deliver(read: index, .success(DetailDelivery(id: id, value: value)))
    }

    func emitError(read index: Int = 0) {
        deliver(
            read: index,
            .failure(
                BridgeError.Diagnostic(
                    category: .internal,
                    detail: "detail load failed"
                )
            )
        )
    }

    private func deliver(
        read index: Int,
        _ result: Result<DetailDelivery<Value>, any Error>
    ) {
        let waiter: CheckedContinuation<DetailDelivery<Value>, any Error>? =
            lock.withLock {
                let read = reads[index]
                if let waiter = read.waiter {
                    read.waiter = nil
                    return waiter
                }
                read.pending.append(result)
                return nil
            }
        waiter?.resume(with: result)
    }
}

@Suite("LibraryStore release detail reader")
struct ReleaseDetailReaderTests {
    @MainActor
    @Test("a read failure surfaces as a per-release error")
    func failureSurfacesError() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let store = LibraryStore()
        let reader = store.releaseDetailReader(
            library: Library(releaseDetail: { feed.query() })
        )

        reader.show("release-1")
        feed.emitError()
        try await Wait.until {
            store.releaseDetailErrors["release-1"] != nil
        }

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.releaseDetailErrors["release-1"] != nil)
    }

    @MainActor
    @Test("a value after a read error is still delivered and clears the error")
    func valueAfterErrorIsDelivered() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let store = LibraryStore()
        let reader = store.releaseDetailReader(
            library: Library(releaseDetail: { feed.query() })
        )

        reader.show("release-1")
        feed.emitError()
        try await Wait.until {
            store.releaseDetailErrors["release-1"] != nil
        }
        feed.emit(id: "release-1", value: makeBridgeRelease())
        try await Wait.until { store.releaseDetails["release-1"] != nil }

        #expect(store.releaseDetailErrors["release-1"] == nil)
    }

    @MainActor
    @Test("an absent value removes the release without inventing an error")
    func absenceRemovesRelease() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let store = LibraryStore()
        store.internReleaseDetail(makeBridgeRelease())
        let reader = store.releaseDetailReader(
            library: Library(releaseDetail: { feed.query() })
        )

        reader.show("release-1")
        feed.emit(id: "release-1", value: nil)
        try await Wait.until { store.releaseDetails["release-1"] == nil }

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }

    @MainActor
    @Test("retry reads again on a fresh read and ends the failed one")
    func retryOpensFreshRead() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let store = LibraryStore()
        let reader = store.releaseDetailReader(
            library: Library(releaseDetail: { feed.query() })
        )

        reader.show("release-1")
        feed.emitError()
        try await Wait.until {
            store.releaseDetailErrors["release-1"] != nil
        }
        reader.retry()
        try await Wait.until { feed.isCancelled(read: 0) }
        feed.emit(read: 1, id: "release-1", value: makeBridgeRelease())
        try await Wait.until { store.releaseDetails["release-1"] != nil }

        #expect(feed.opened == 2)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }

    @MainActor
    @Test("showing another release moves the one read")
    func anotherReleaseMovesTheRead() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let store = LibraryStore()
        let reader = store.releaseDetailReader(
            library: Library(releaseDetail: { feed.query() })
        )

        reader.show("release-1")
        reader.show("release-2")
        feed.emit(id: "release-1", value: makeBridgeRelease())
        feed.emit(id: "release-2", value: nil)
        // One ask on opening, one past each of the two values.
        try await Wait.until { feed.asks() >= 3 }

        #expect(feed.opened == 1)
        #expect(feed.requested == ["release-1", "release-2"])
        #expect(
            store.releaseDetails["release-1"] == nil,
            "a value for the release no longer shown is not applied"
        )
    }
}
