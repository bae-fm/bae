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
    format: String? = "FLAC",
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
        displayName: displayName,
        year: 2024,
        format: format,
        label: nil,
        catalogNumber: nil,
        country: nil,
        storageState: storageState,
        pinned: pinned,
        storageActions: storageActions,
        transferAction: transferAction,
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

final class TestLiveSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
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

// MARK: - Release-detail subscription errors and values

private final class DetailSubscriptionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var calls = 0
    private let failFirst: Int
    private let release: BridgeRelease?

    init(failFirst: Int, release: BridgeRelease?) {
        self.failFirst = failFirst
        self.release = release
    }

    func subscribe(
        callback: ReleaseDetailCallback
    ) -> any LiveSubscriptionProtocol {
        lock.lock()
        calls += 1
        let shouldFail = calls <= failFirst
        let release = release
        lock.unlock()
        return TestLiveSubscription(
            Task {
                if shouldFail {
                    callback.onError(
                        error: .Diagnostic(
                            category: .internal,
                            detail: "detail load failed"
                        )
                    )
                }
                else {
                    callback.onValue(value: release)
                }
            }
        )
    }
}

private final class ContinuingDetailSubscriptionProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var callback: ReleaseDetailCallback?

    func subscribe(
        callback: ReleaseDetailCallback
    ) -> any LiveSubscriptionProtocol {
        lock.withLock {
            self.callback = callback
        }
        return TestLiveSubscription(Task {})
    }

    func emitError() {
        let callback: (any ReleaseDetailCallback)? = lock.withLock {
            self.callback
        }
        callback?
            .onError(
                error: BridgeError.Diagnostic(
                    category: BridgeErrorCategory.internal,
                    detail: "detail load failed"
                )
            )
    }

    func emitValue(_ value: BridgeRelease?) {
        let callback: (any ReleaseDetailCallback)? = lock.withLock {
            self.callback
        }
        callback?.onValue(value: value)
    }
}

@MainActor
func waitForStoreUpdate(_ condition: () -> Bool) async {
    for _ in 0..<100 where !condition() {
        await Task.yield()
    }
}

@Suite("LibraryStore.observeReleaseDetail")
struct ObserveReleaseDetailTests {

    @MainActor
    @Test(
        "a subscription failure surfaces as a per-release error"
    )
    func failureSurfacesError() async {
        let probe = ContinuingDetailSubscriptionProbe()
        let store = LibraryStore()
        let library = Library(subscribeReleaseDetail: { _, callback in
            probe.subscribe(callback: callback)
        })

        let observation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: library,
                onValue: {}
            )
        }
        await Task.yield()
        probe.emitError()
        await waitForStoreUpdate {
            store.releaseDetailErrors["release-1"] != nil
        }

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.releaseDetailErrors["release-1"] != nil)

        observation.cancel()
        probe.emitValue(nil)
        await observation.value
    }

    @MainActor
    @Test("a value after a live error is still delivered")
    func valueAfterErrorIsDelivered() async {
        let probe = ContinuingDetailSubscriptionProbe()
        let store = LibraryStore()
        let library = Library(subscribeReleaseDetail: { _, callback in
            probe.subscribe(callback: callback)
        })

        let observation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: library,
                onValue: {}
            )
        }
        await Task.yield()
        probe.emitError()
        await waitForStoreUpdate {
            store.releaseDetailErrors["release-1"] != nil
        }
        probe.emitValue(makeBridgeRelease())
        await waitForStoreUpdate { store.releaseDetails["release-1"] != nil }
        observation.cancel()
        probe.emitValue(nil)
        await observation.value

        #expect(store.releaseDetails["release-1"] != nil)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }

    @MainActor
    @Test("an absent live value removes the release without inventing an error")
    func absenceRemovesRelease() async {
        let probe = DetailSubscriptionProbe(failFirst: 0, release: nil)
        let store = LibraryStore()
        store.internReleaseDetail(makeBridgeRelease())
        let library = Library(subscribeReleaseDetail: { _, callback in
            probe.subscribe(callback: callback)
        })

        let observation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: library,
                onValue: {}
            )
        }
        await waitForStoreUpdate { store.releaseDetails["release-1"] == nil }
        observation.cancel()
        await observation.value

        #expect(store.releaseDetails["release-1"] == nil)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }

    @MainActor
    @Test("a new subscription after failure clears the error with its value")
    func retryClearsErrorAndLoads() async {
        let failingProbe = ContinuingDetailSubscriptionProbe()
        let store = LibraryStore()
        let failingLibrary = Library(subscribeReleaseDetail: { _, callback in
            failingProbe.subscribe(callback: callback)
        })

        let failedObservation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: failingLibrary,
                onValue: {}
            )
        }
        await Task.yield()
        failingProbe.emitError()
        await waitForStoreUpdate {
            store.releaseDetailErrors["release-1"] != nil
        }
        #expect(store.releaseDetailErrors["release-1"] != nil)
        #expect(store.releaseDetails["release-1"] == nil)
        failedObservation.cancel()
        failingProbe.emitValue(nil)
        await failedObservation.value

        let succeedingProbe = DetailSubscriptionProbe(
            failFirst: 0,
            release: makeBridgeRelease()
        )
        let succeedingLibrary = Library(subscribeReleaseDetail: { _, callback in
            succeedingProbe.subscribe(callback: callback)
        })

        let observation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: succeedingLibrary,
                onValue: {}
            )
        }
        await waitForStoreUpdate { store.releaseDetails["release-1"] != nil }
        observation.cancel()
        await observation.value
        #expect(store.releaseDetailErrors["release-1"] == nil)
        #expect(store.releaseDetails["release-1"] != nil)
    }

    @MainActor
    @Test("an initial live value leaves no error")
    func successLeavesNoError() async {
        let store = LibraryStore()
        let probe = DetailSubscriptionProbe(
            failFirst: 0,
            release: makeBridgeRelease()
        )
        let library = Library(subscribeReleaseDetail: { _, callback in
            probe.subscribe(callback: callback)
        })

        let observation = Task {
            await store.observeReleaseDetail(
                releaseId: "release-1",
                library: library,
                onValue: {}
            )
        }
        await waitForStoreUpdate { store.releaseDetails["release-1"] != nil }
        observation.cancel()
        await observation.value

        #expect(store.releaseDetails["release-1"] != nil)
        #expect(store.releaseDetailErrors["release-1"] == nil)
    }
}
