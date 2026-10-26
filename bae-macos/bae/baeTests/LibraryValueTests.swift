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
func makeBridgeRelease(
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
    let facts = PreviewData.pressingFacts(media: media)
    return BridgeRelease(
        id: id,
        albumId: albumId,
        name: .named(name: displayName),
        year: 2024,
        labels: [],
        facts: facts,
        pressingSummary: bridgePressingSummary(facts: facts),
        mediaTerms: bridgeMediaTerms(media: facts.media),
        pressingDetails: bridgePressingDetails(facts: facts),
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
        playability: .playable,
        downloadStatus: nil,
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
        mediaTerms: bridgeMediaTerms(media: media),
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: nil,
        fileCount: fileCount,
        totalSize: totalSize,
        cover: cover
    )
}

/// An album detail as core reads one: never without a release, its primary
/// release first.
func makeBridgeAlbumDetail(
    albumId: String = "album-1",
    title: String = "Album Title",
    releases: [BridgeRelease] = [makeBridgeRelease()]
) -> BridgeAlbumDetail {
    BridgeAlbumDetail(
        album: makeBridgeAlbum(
            id: albumId,
            title: title,
            releaseIds: releases.map(\.id),
            primaryReleaseId: releases[0].id
        ),
        releases: releases,
    )
}

@MainActor
func makeList(albums: [BridgeAlbum]) -> AlbumList {
    AlbumList(
        pageSource: AlbumPreviewPageSource(albums: albums),
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

/// Test-only page source that counts subscriptions. Used to pin that a live
/// page is not asked for again — the rows a page holds are the same either
/// way, so a naive idempotency assertion passes whether or not another page
/// query was actually subscribed.
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

// MARK: - Release and album values

@Suite("ReleaseSummary")
struct ReleaseSummaryTests {
    @MainActor
    @Test("a summary from a list's row carries its fields")
    func fromListRow() {
        let summary = ReleaseSummary(
            from: makeBridgeReleaseSummary(
                pinned: true,
                totalSize: 200,
                cover: BridgeImageRef(
                    id: "release-1",
                    version: "2",
                    imageType: .cover
                )
            )
        )

        #expect(summary.pinned)
        #expect(summary.totalSize == 200)
        #expect(
            summary.cover
                == BridgeImageRef(
                    id: "release-1",
                    version: "2",
                    imageType: .cover
                )
        )
    }

    @MainActor
    @Test("a summary from a detail's release carries its fields")
    func fromDetailRelease() {
        let summary = ReleaseSummary(
            from: makeBridgeRelease(
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
        )

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
    @Test("a release mid-transition carries its transfer")
    func carriesTransfer() {
        let summary = ReleaseSummary(
            from: makeBridgeRelease(transferAction: .upload)
        )

        #expect(summary.transfer != nil)
    }
}

@Suite("ReleaseDetail and AlbumDetail")
struct ReleaseDetailTests {
    @MainActor
    @Test("a detail carries the fat fields and its summary's slim ones")
    func detailCarriesFatFields() {
        let detail = ReleaseDetail(
            bridge: makeBridgeRelease(displayName: "Deluxe Edition")
        )

        #expect(detail.displayName == "Deluxe Edition")
        #expect(detail.totalDuration == .hoursAndMinutes(hours: 1, minutes: 45))
        #expect(detail.summary.media == PreviewData.media(.cd))
    }

    @MainActor
    @Test("an album detail holds the album and each release in album order")
    func albumDetailHoldsItsReleases() {
        let detail = AlbumDetail(
            bridge: makeBridgeAlbumDetail(
                releases: [
                    makeBridgeRelease(id: "r-1"),
                    makeBridgeRelease(id: "r-2", displayName: "Release Two"),
                ]
            )
        )

        #expect(detail.album.id == "album-1")
        #expect(detail.releases.map(\.id) == ["r-1", "r-2"])
        #expect(detail.release("r-2")?.displayName == "Release Two")
        #expect(detail.release("r-3") == nil)
        #expect(detail.primaryRelease.id == "r-1")
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

/// What a detail reader handed on: each value, and each failure.
@MainActor
private final class DetailSink<Value: Sendable> {
    private(set) var values: [(id: String, value: Value?)] = []
    private(set) var errors: [String] = []

    func reader(
        _ open: @escaping () -> DetailQuery<Value>
    ) -> DetailReader<Value> {
        DetailReader(
            open: open,
            onValue: { [weak self] id, value in
                self?.values.append((id, value))
            },
            onError: { [weak self] id, _ in self?.errors.append(id) }
        )
    }
}

@Suite("DetailReader")
struct DetailReaderTests {
    @MainActor
    @Test("a read failure is handed on against the item shown")
    func failureIsHandedOn() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let sink = DetailSink<BridgeRelease>()
        let reader = sink.reader { feed.query() }

        reader.show("release-1")
        feed.emitError()
        try await Wait.until { !sink.errors.isEmpty }

        #expect(sink.errors == ["release-1"])
        #expect(sink.values.isEmpty)
    }

    @MainActor
    @Test("a value after a read error is still delivered")
    func valueAfterErrorIsDelivered() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let sink = DetailSink<BridgeRelease>()
        let reader = sink.reader { feed.query() }

        reader.show("release-1")
        feed.emitError()
        try await Wait.until { !sink.errors.isEmpty }
        feed.emit(id: "release-1", value: makeBridgeRelease())
        try await Wait.until { !sink.values.isEmpty }

        #expect(sink.values.map(\.id) == ["release-1"])
        #expect(sink.values.first?.value != nil)
    }

    @MainActor
    @Test("an absent item is delivered as absent, not as a failure")
    func absenceIsDelivered() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let sink = DetailSink<BridgeRelease>()
        let reader = sink.reader { feed.query() }

        reader.show("release-1")
        feed.emit(id: "release-1", value: nil)
        try await Wait.until { !sink.values.isEmpty }

        #expect(sink.values.first?.id == "release-1")
        #expect(sink.values.first?.value == nil)
        #expect(sink.errors.isEmpty)
    }

    @MainActor
    @Test(
        "retry reads again on a fresh read and rejects the failed one's value"
    )
    func retryRejectsOldRead() async throws {
        let feed = DetailFeed<BridgeAlbumDetail>()
        let sink = DetailSink<BridgeAlbumDetail>()
        let reader = sink.reader { feed.query() }

        reader.show("album-1")
        feed.emitError()
        try await Wait.until { !sink.errors.isEmpty }

        reader.retry()
        try await Wait.until { feed.isCancelled(read: 0) }
        #expect(feed.opened == 2)
        feed.emit(
            read: 1,
            id: "album-1",
            value: makeBridgeAlbumDetail(title: "Replacement Title")
        )
        try await Wait.until { !sink.values.isEmpty }
        feed.emit(
            read: 0,
            id: "album-1",
            value: makeBridgeAlbumDetail(title: "Old Title")
        )
        // Read 0 was cancelled above and its loop has ended, so nothing will
        // ever take this value: the check below is final, not early.

        #expect(sink.values.map(\.value?.album.title) == ["Replacement Title"])

        reader.close()
        try await Wait.until { feed.isCancelled(read: 1) }
        #expect(feed.isCancelled(read: 1))
    }

    @MainActor
    @Test("showing another item moves the one read")
    func anotherItemMovesTheRead() async throws {
        let feed = DetailFeed<BridgeRelease>()
        let sink = DetailSink<BridgeRelease>()
        let reader = sink.reader { feed.query() }

        reader.show("release-1")
        reader.show("release-2")
        feed.emit(id: "release-1", value: makeBridgeRelease())
        feed.emit(id: "release-2", value: nil)
        // One ask on opening, one past each of the two values.
        try await Wait.until { feed.asks() >= 3 }

        #expect(feed.opened == 1)
        #expect(feed.requested == ["release-1", "release-2"])
        #expect(
            sink.values.map(\.id) == ["release-2"],
            "a value for the item no longer shown is not handed on"
        )
    }
}
