import BaeKit
import Foundation
import Testing

@testable import bae

private func makeCandidate(_ key: String) -> Candidate {
    Candidate(
        reIdentifyKey: key,
        releaseId: "existing-release",
        displayName: "Candidate \(key)"
    )
}

private func makeStatus(albumId: String) -> BridgeLibraryStatus {
    BridgeLibraryStatus(
        releaseId: "unused",
        releaseInLibrary: true,
        albumInLibrary: true,
        albumTitle: "Album Title",
        albumId: albumId
    )
}

// MARK: - Bridge snapshot builders

/// The Bridge* records are generated at build from `bae-bridge/src/types.rs`;
/// these hand-build the minimal shapes the snapshot reducers consume.

private func emptyBridgeFiles() -> BridgeCandidateFiles {
    BridgeCandidateFiles(files: [], formatLabel: "", collapsedDirectories: [])
}

private func bridgeFolder(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    trackCount: UInt32 = 10,
    skipped: Bool = false,
    isAdded: Bool = false
) -> BridgeFolderCandidate {
    BridgeFolderCandidate(
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        files: emptyBridgeFiles(),
        trackCount: trackCount,
        skipped: skipped,
        isAdded: isAdded
    )
}

/// A folder-source `Candidate` with the given scan flags, for the
/// batch/single-snapshot suites.
private func folderCandidate(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    isAdded: Bool = false
) -> Candidate {
    Candidate(
        bridge: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped,
            isAdded: isAdded
        )
    )
}

private func bridgeInvalid(
    folderPath: String,
    watchedFolderPath: String,
    name: String
) -> BridgeInvalidCandidate {
    BridgeInvalidCandidate(
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        displayPath: name,
        resolvedBoundaries: [],
        reason: .noValidAudio
    )
}

// MARK: - Triage row builders

private func matchedRelease(
    releaseId: String,
    title: String,
    trackCount: UInt32? = 10,
    coverThumbnailUrl: String? = nil
) -> BridgeMatchedRelease {
    BridgeMatchedRelease(
        releaseId: releaseId,
        title: title,
        artist: "Artist",
        pressing: trackCount.map {
            BridgeMatchedPressing(year: 2000, format: "CD", trackCount: $0)
        },
        coverThumbnailUrl: coverThumbnailUrl,
        evidence: BridgeMatchEvidence(source: .musicBrainz, signal: .discId)
    )
}

/// A Ready row: matched, selectable, no import status.
private func readyRow(
    _ key: String,
    title: String,
    coverThumbnailUrl: String? = nil,
    metadataSummary: BridgeTriageMetadataSummary? = nil
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .ready,
        skipAction: .skip,
        matched: matchedRelease(
            releaseId: "rel-\(key)",
            title: title,
            coverThumbnailUrl: coverThumbnailUrl
        ),
        metadataSummary: metadataSummary,
        selectable: true,
        importStatus: nil,
        metadataProvenance: .externalRelease(
            source: .musicBrainz,
            releaseId: "rel-\(key)"
        )
    )
}

/// A Needs-you row in `group`, never selectable.
private func needsYouRow(
    _ key: String,
    title: String,
    group: BridgeNeedsYouGroup,
    reason: BridgeNeedsYouReason
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .needsYou(group: group, reason: reason),
        skipAction: .skip,
        matched: nil,
        metadataSummary: nil,
        selectable: false,
        importStatus: nil,
        metadataProvenance: nil
    )
}

private func doneRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .done,
        skipAction: nil,
        matched: matchedRelease(releaseId: "rel-\(key)", title: title),
        metadataSummary: nil,
        selectable: false,
        importStatus: .complete(releaseId: "rel-\(key)", albumId: "al-\(key)"),
        metadataProvenance: nil
    )
}

private func skippedRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        resolvedBoundaries: [],
        combineAncestorKey: nil,
        actionable: true,
        placement: .skipped,
        skipAction: .unskip,
        matched: nil,
        metadataSummary: nil,
        selectable: false,
        importStatus: nil,
        metadataProvenance: nil
    )
}

private func detail(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    resumedIdentifyState: BridgeIdentifyState = .idle,
    row: BridgeTriageRow? = nil,
    cover: BridgeCoverChoice? = nil,
    release: BridgeReleaseDetail? = nil
) -> BridgeImportCandidateDetail {
    BridgeImportCandidateDetail(
        candidate: bridgeFolder(
            folderPath: folderPath,
            watchedFolderPath: watchedFolderPath,
            name: name,
            skipped: skipped
        ),
        actionable: true,
        resumedIdentifyState: resumedIdentifyState,
        row: row ?? readyRow(folderPath, title: name),
        release: release,
        pickedLibraryStatus: nil,
        fileEvidence: [],
        metadataDraft: MappingFixtures.albumEdit,
        metadataDraftIsBlank: false,
        metadataProvenance: MappingFixtures.provenance,
        metadataRevision: 1,
        initialMetadataSource: .none,
        mapping: BridgeMappingTable(
            images: [],
            rows: [],
            reconciliation: nil
        ),
        unprobed: [],
        cover: cover,
        signals: nil,
        failure: nil
    )
}

@Suite("ImportStore per-candidate reads")
struct ImportStoreCandidateDetailTests {
    @MainActor
    @Test("a read installs the folder, its resumed state and its row")
    func installsTheRead() throws {
        let store = ImportStore()

        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                resumedIdentifyState: .notFoundAnywhere
            )
        )

        let read = try #require(store.selectedCandidates["/w1/a"])
        #expect(read.displayName == "A")
        // With no run live the resumed state is what the pane shows.
        #expect(read.resumedIdentifyState == .notFoundAnywhere)
        #expect(read.row?.candidateKey == "/w1/a")
        #expect(read.row?.importStatus == nil)
    }

    @MainActor
    @Test("a re-read keeps the editor state on its key")
    func keepsEditorState() throws {
        let store = ImportStore()
        var existing = folderCandidate(
            folderPath: "/w1/a",
            watchedFolderPath: "/w1",
            name: "A"
        )
        existing.libraryStatuses = ["rel-1": makeStatus(albumId: "al-1")]
        existing.error = "the last command failed"
        let pendingPick = BridgeMetadataProvenance.externalRelease(
            source: .musicBrainz,
            releaseId: "rel-1"
        )
        existing.metadataApplicationSession =
            CandidateMetadataApplicationSession(
                provenance: pendingPick
            )
        existing.metadataPresentation = .fileTags
        existing.fileTagsPreview = .loaded(MappingFixtures.albumSeed)
        existing.search.searchAlbum = "typed album"
        store.selectedCandidates["/w1/a"] = existing

        // Same key, renamed + skip flipped.
        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A-renamed",
                skipped: true
            )
        )

        let merged = try #require(store.selectedCandidates["/w1/a"])
        // The user's work on this pane survives; the read only re-read the
        // folder.
        #expect(merged.libraryStatuses["rel-1"] != nil)
        #expect(merged.error == "the last command failed")
        #expect(merged.provenanceInFlight == pendingPick)
        #expect(merged.metadataPresentation == .fileTags)
        #expect(merged.fileTagsPreview.edit == MappingFixtures.albumSeed)
        #expect(merged.search.searchAlbum == "typed album")
        // Scan fields come from the incoming read.
        #expect(merged.displayName == "A-renamed")
        #expect(merged.files.files.isEmpty)
    }
}

/// The one rule about which identify state a surface shows. It used to live on
/// `Candidate`, reconciling two stored fields; the run in flight is not stored
/// any more, so the rule is a function of the two values a surface holds.
@Suite("The identify state a candidate shows")
struct ShownIdentifyStateTests {
    private func runtime(
        _ state: BridgeIdentifyState
    ) -> BridgeCandidateRuntimeSnapshot {
        BridgeCandidateRuntimeSnapshot(
            identifyState: state,
            signalsToolbar: BridgeSignalsToolbar(signals: []),
            import: nil
        )
    }

    @Test("a live run outranks the stored verdict's resumed state")
    func liveRunWins() {
        let shown = shownIdentifyState(
            resumed: .notFoundAnywhere,
            runtime: runtime(
                .triangulating(discid: .computing, barcode: .scanning)
            )
        )
        #expect(shown == .triangulating(discid: .computing, barcode: .scanning))
    }

    @Test("nothing running leaves the resumed state")
    func nothingRunning() {
        #expect(
            shownIdentifyState(resumed: .notFoundAnywhere, runtime: nil)
                == .notFoundAnywhere
        )
    }

    @Test("a run that is idle leaves the resumed state")
    func idleRunDefersToTheVerdict() {
        #expect(
            shownIdentifyState(
                resumed: .notFoundAnywhere,
                runtime: runtime(.idle)
            ) == .notFoundAnywhere
        )
    }
}

@Suite("ImportStore sidebar covers")
struct ImportStoreSidebarCoverTests {
    @MainActor
    @Test("applied row covers remain after deselection")
    func appliedCoversSurviveDeselection() throws {
        let store = ImportStore()
        let key = "/w/subject"
        let remoteCover = try #require(PreviewData.remoteCovers.last)
        let localArtwork = try #require(
            PreviewData.bridgeCandidateFiles.images.last
        )
        let choices = [
            remoteCover.coverChoice,
            try #require(localArtwork.coverChoice),
        ]
        for choice in choices {
            let row = readyRow(
                key,
                title: "Subject",
                coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg",
                metadataSummary: BridgeTriageMetadataSummary(
                    albumTitle: "Applied Draft",
                    albumArtistAssignments: [],
                    coverThumbnail: choice.thumbnailSource
                )
            )
            store.applyCandidateDetail(
                key: key,
                detail: detail(
                    folderPath: key,
                    watchedFolderPath: "/w",
                    name: "Subject",
                    row: row,
                    cover: choice
                )
            )
            store.selectedCandidates.removeValue(forKey: key)

            #expect(
                store.sidebarCover(for: row)
                    == ImageContent(bridge: choice.thumbnailSource)
            )
        }
    }

    @Test(
        "the sidebar retains the queue thumbnail before a candidate has a cover"
    )
    func queueThumbnailRendersBeforeCoverResolution() {
        let row = readyRow(
            "/w/subject",
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )

        #expect(
            ImportStore().sidebarCover(for: row)
                == .remote(url: "https://example.com/queue-thumbnail.jpg")
        )
    }
}

// MARK: - The paged list

/// How many pages the paged list has taken delivery of.
@MainActor
private final class DeliveredPages {
    var count = 0
}

@MainActor
private final class CandidatePositionOutcome {
    enum State: Equatable {
        case waiting
        case resolved(Int?)
        case failed
    }

    var state = State.waiting
}

@Suite("Import list page source")
struct ImportListPageSourceTests {
    private struct PositionReadFailed: Error {}

    private struct SnapshotWindow {
        let window: BridgeLibraryPageWindow
        let keys: [String]

        init(offset: UInt64, limit: UInt64, keys: [String]) {
            self.window = BridgeLibraryPageWindow(
                offset: offset,
                limit: limit
            )
            self.keys = keys
        }
    }

    /// A stub bridge subscription: it records the windows asked for and hands
    /// back the values a test queues.
    private final class StubListSubscription: ImportListSubscriptionProtocol,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var windows: [[BridgeLibraryPageWindow]] = []
        private var pending: [Result<BridgeImportListSnapshot, any Error>] = []
        private var viewRevision: UInt64 = 0
        private var views: [BridgeImportListView] = []
        private var setViewHook: (() -> Void)?
        private var waiter:
            CheckedContinuation<BridgeImportListSnapshot, any Error>?

        var requestedWindows: [[BridgeLibraryPageWindow]] {
            lock.lock()
            defer { lock.unlock() }
            return windows
        }

        var requestedViews: [BridgeImportListView] {
            lock.lock()
            defer { lock.unlock() }
            return views
        }

        func setWindows(windows: [BridgeLibraryPageWindow]) throws {
            lock.lock()
            self.windows.append(windows)
            lock.unlock()
        }

        func setView(view: BridgeImportListView) throws -> UInt64 {
            lock.lock()
            viewRevision += 1
            views.append(view)
            let revision = viewRevision
            let hook = setViewHook
            lock.unlock()
            hook?()
            return revision
        }

        func onSetView(_ hook: @escaping () -> Void) {
            lock.withLock { setViewHook = hook }
        }

        func cancel() async throws {}

        func deliver(_ snapshot: BridgeImportListSnapshot) {
            lock.lock()
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(.success(snapshot)) }
            lock.unlock()
            waiter?.resume(returning: snapshot)
        }

        func fail(_ error: any Error) {
            lock.lock()
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(.failure(error)) }
            lock.unlock()
            waiter?.resume(throwing: error)
        }

        func next() async throws -> BridgeImportListSnapshot {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                if pending.isEmpty {
                    waiter = continuation
                    lock.unlock()
                }
                else {
                    let snapshot = pending.removeFirst()
                    lock.unlock()
                    continuation.resume(with: snapshot)
                }
            }
        }
    }

    private func item(_ key: String) -> BridgeImportListItem {
        .candidate(
            stableKey: "candidate:\(key)",
            row: readyRow(key, title: key),
            isGroupMember: false
        )
    }

    private func snapshot(
        _ windows: [SnapshotWindow],
        totalCount: UInt64,
        firstUnidentifiedCandidateKey: String? = nil,
        firstUnidentifiedPosition: UInt64? = nil,
        requestRevision: UInt64 = 0
    ) -> BridgeImportListSnapshot {
        BridgeImportListSnapshot(
            windows: windows.map { fixture in
                BridgeImportListWindow(
                    window: fixture.window,
                    items: fixture.keys.map(item)
                )
            },
            totalCount: totalCount,
            summary: BridgeImportQueueSummary(
                counts: BridgeTriageTabCounts(
                    pending: UInt32(totalCount),
                    done: 0,
                    skipped: 0
                ),
                watchedFolders: [BridgeWatchedFolder(path: "/w", name: "w")],
                folderScanStatuses: [],
                groupKeys: [],
                ready: [],
                firstUnidentified: firstUnidentifiedCandidateKey.map {
                    BridgeFirstUnidentifiedRowRef(
                        candidateKey: $0,
                        stableKey: "candidate:\($0)",
                        groupKey: nil,
                        visiblePosition: firstUnidentifiedPosition
                    )
                }
            ),
            requestRevision: requestRevision,
            cause: .requestChanged
        )
    }
}

extension ImportListPageSourceTests {
    @MainActor
    @Test("candidate position waits for the view revision that defines it")
    func candidatePositionWaitsForItsViewRevision() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        let view = BridgeImportListView(
            tab: .pending,
            filterText: "",
            collapsedGroups: [],
            order: .pathAscending
        )
        let outcome = CandidatePositionOutcome()
        Task {
            do {
                outcome.state = .resolved(
                    try await source.pages.firstUnidentifiedPosition(
                        for: BridgeFirstUnidentifiedRowRef(
                            candidateKey: "/w/target",
                            stableKey: "candidate:/w/target",
                            groupKey: nil,
                            visiblePosition: nil
                        ),
                        afterApplying: view
                    )
                )
            }
            catch {
                outcome.state = .failed
            }
        }
        await settle(until: { subscription.requestedViews == [view] })

        subscription.deliver(
            snapshot(
                [],
                totalCount: 70,
                firstUnidentifiedCandidateKey: "/w/target",
                firstUnidentifiedPosition: 4,
                requestRevision: 0
            )
        )
        await Task.yield()
        #expect(outcome.state == .waiting)

        subscription.deliver(
            snapshot(
                [],
                totalCount: 70,
                firstUnidentifiedCandidateKey: "/w/target",
                firstUnidentifiedPosition: 61,
                requestRevision: 1
            )
        )
        await settle(until: { outcome.state != .waiting })
        #expect(outcome.state == .resolved(61))
    }

    @MainActor
    @Test("candidate position cancellation cannot miss registration")
    func candidatePositionCancellationAtRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        let task = Task {
            try await source.pages.firstUnidentifiedPosition(
                for: BridgeFirstUnidentifiedRowRef(
                    candidateKey: "/w/target",
                    stableKey: "candidate:/w/target",
                    groupKey: nil,
                    visiblePosition: nil
                ),
                afterApplying: BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        task.cancel()

        do {
            _ = try await task.value
            Issue.record("a cancelled position wait returned a position")
        }
        catch is CancellationError {}
        catch {
            Issue.record("unexpected cancellation error: \(error)")
        }
    }

    @MainActor
    @Test("candidate position registration receives a source failure")
    func candidatePositionFailureAtRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        let task = Task {
            try await source.pages.firstUnidentifiedPosition(
                for: BridgeFirstUnidentifiedRowRef(
                    candidateKey: "/w/target",
                    stableKey: "candidate:/w/target",
                    groupKey: nil,
                    visiblePosition: nil
                ),
                afterApplying: BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }
        await settle(until: { !subscription.requestedViews.isEmpty })

        subscription.fail(PositionReadFailed())

        do {
            _ = try await task.value
            Issue.record("a failed position wait returned a position")
        }
        catch is PositionReadFailed {}
        catch {
            Issue.record("unexpected position error: \(error)")
        }
    }

    @Test("a failure between view acceptance and registration is retained")
    func candidatePositionFailureBeforeRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        let failureDelivered = DispatchSemaphore(value: 0)
        _ = source.subscribe(
            offset: 0,
            limit: 1,
            onValue: { _, _ in },
            onError: { _ in failureDelivered.signal() }
        )
        subscription.onSetView {
            subscription.fail(PositionReadFailed())
            failureDelivered.wait()
        }
        let pages = source.pages
        let task = Task {
            try await pages.firstUnidentifiedPosition(
                for: BridgeFirstUnidentifiedRowRef(
                    candidateKey: "/w/target",
                    stableKey: "candidate:/w/target",
                    groupKey: nil,
                    visiblePosition: nil
                ),
                afterApplying: BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        do {
            _ = try await task.value
            Issue.record("a failed position wait returned a position")
        }
        catch is PositionReadFailed {}
        catch {
            Issue.record("unexpected position error: \(error)")
        }
    }

    @MainActor
    @Test("one value updates every registered page, and its summary")
    func oneValueUpdatesEveryPage() async throws {
        let subscription = StubListSubscription()
        var summaries: [BridgeImportQueueSummary] = []
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { summaries.append($0) }
        )
        var first: [String] = []
        var second: [String] = []
        var totals: [Int] = []
        _ = source.subscribe(
            offset: 0,
            limit: 2,
            onValue: { items, total in
                first = items.map(\.id)
                totals.append(total)
            },
            onError: { _ in }
        )
        _ = source.subscribe(
            offset: 2,
            limit: 2,
            onValue: { items, total in
                second = items.map(\.id)
                totals.append(total)
            },
            onError: { _ in }
        )

        subscription.deliver(
            snapshot(
                [
                    SnapshotWindow(
                        offset: 0,
                        limit: 2,
                        keys: ["/w/a", "/w/b"]
                    ),
                    SnapshotWindow(offset: 2, limit: 2, keys: ["/w/c"]),
                ],
                totalCount: 3,
                firstUnidentifiedCandidateKey: "/w/c"
            )
        )
        await settle(until: { totals.count == 2 })

        #expect(first == ["candidate:/w/a", "candidate:/w/b"])
        #expect(second == ["candidate:/w/c"])
        #expect(totals == [3, 3])
        #expect(
            summaries.last?.firstUnidentified?.candidateKey == "/w/c"
        )
    }

    @MainActor
    @Test("cancelling one page asks for the windows that are left")
    func cancellingOnePageShrinksTheWindows() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        _ = source.subscribe(
            offset: 0,
            limit: 2,
            onValue: { _, _ in },
            onError: { _ in }
        )
        let second = source.subscribe(
            offset: 2,
            limit: 2,
            onValue: { _, _ in },
            onError: { _ in }
        )

        #expect(
            subscription.requestedWindows.last?.map(\.offset) == [0, 2]
        )

        second.cancel()
        #expect(subscription.requestedWindows.last?.map(\.offset) == [0])
    }

    @MainActor
    @Test("a redelivered window and a new one both keep every loaded item")
    func deliveriesNeverDropALoadedItem() async throws {
        let total = 60
        let keys = (0..<total).map { String(format: "/w/%03d", $0) }
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in }
        )
        let importStore = ImportStore()
        // A redelivered page changes nothing else about the list, so the count
        // of pages taken is the one place a delivery is observable — and the
        // only thing each wait below can honestly wait for.
        let delivered = DeliveredPages()
        let list = PaginatedList<BridgeImportListItem>(
            pageSource: source,
            ingest: {
                delivered.count += 1
                importStore.ingest($0)
            },
            onError: { _ in },
            onSnapshot: { ids, _ in importStore.retainItems(ids) }
        )
        let firstPage = Array(keys[0..<50])
        let secondPage = Array(keys[50..<60])

        async let initial: Void = list.loadInitial()
        await settle(until: { !subscription.requestedWindows.isEmpty })
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        await initial
        await settle(until: { delivered.count == 1 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        // A commit that leaves this window's rows exactly where they were —
        // an identification tick on a row further down the queue. The value
        // repeats, and nothing it repeats may go missing.
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        await settle(until: { delivered.count == 2 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        // Scrolling past the page boundary registers a second window. The
        // first window's rows are still on screen, so they stay resolvable
        // while the value that answers the new window is still in flight.
        async let next: Void = list.loadPage(containing: 55)
        await settle(until: { subscription.requestedWindows.last?.count == 2 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        subscription.deliver(
            snapshot(
                [
                    SnapshotWindow(offset: 0, limit: 50, keys: firstPage),
                    SnapshotWindow(offset: 50, limit: 50, keys: secondPage),
                ],
                totalCount: UInt64(total)
            )
        )
        await next
        // One page taken per window, so the two-window value lands as two.
        await settle(until: { delivered.count == 4 })
        #expect(loadedKeys(list, importStore, 0..<60) == keys)
    }

    /// The keys the list holds at `positions`, resolved the way a row does:
    /// the position's id, then that id's item in the store. A position either
    /// side of that misses and renders as an empty row.
    @MainActor
    private func loadedKeys(
        _ list: PaginatedList<BridgeImportListItem>,
        _ importStore: ImportStore,
        _ positions: Range<Int>
    ) -> [String] {
        positions.compactMap { position in
            guard let id = list.idAt(position),
                let item = importStore.items[id]
            else { return nil }
            switch item {
            case .candidate(_, let row, _): return row.candidateKey
            case .groupHeader, .invalid: return nil
            }
        }
    }

    /// Let the source's delivery task reach the main actor, waiting for the
    /// thing being waited on rather than for a fixed number of turns.
    ///
    /// A delivery hops off the main actor and back, and how many turns that
    /// takes depends on what else the process is running — a fixed count of
    /// yields passes with this suite alone and loses the race in a loaded test
    /// bundle, where it reads as a value that never arrived.
    @MainActor
    private func settle(until arrived: @MainActor () -> Bool) async {
        for _ in 0..<100 where !arrived() {
            await Task.yield()
        }
    }
}
