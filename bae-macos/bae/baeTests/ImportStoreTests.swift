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

/// Hand-built bridge records for the snapshot reducers.

private func emptyBridgeFiles() -> BridgeCandidateFiles {
    BridgeCandidateFiles(
        fileMetadataIdentity: "empty-audio-files",
        files: [],
        coverFiles: [],
        sourceAudio: nil
    )
}

private func bridgeFiles(fileMetadataIdentity: String) -> BridgeCandidateFiles {
    BridgeCandidateFiles(
        fileMetadataIdentity: fileMetadataIdentity,
        files: [
            BridgeCandidateFile(
                file: BridgeFileInfo(
                    name: "01.flac",
                    size: 100,
                    dirPrefix: nil,
                    fileName: "01.flac",
                    localPath: "/music/01.flac",
                    audioFormat: nil
                ),
                role: .audio,
                becomes: .slots(first: 1, last: 1),
                alternatives: [.audio, .notATrack],
                roleChoice: .audio
            )
        ],
        coverFiles: [],
        sourceAudio: nil
    )
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
        parts: [],
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        files: emptyBridgeFiles(),
        trackCount: trackCount,
        skipped: skipped,
        isAdded: isAdded
    )
}

/// A folder `Candidate` with the given scan flags.
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
        candidateKey: folderPath,
        folderPath: folderPath,
        sourceFolderName: name,
        watchedFolderPath: watchedFolderPath,
        displayPath: name,
        separable: false,
        reason: .noValidAudio
    )
}

// MARK: - Triage row builders

private func matchedRelease(
    releaseId: String,
    title: String,
    trackCount: UInt32? = 10,
    cover: BridgeRemoteImageSet? = nil
) -> BridgeMatchedRelease {
    BridgeMatchedRelease(
        releaseId: releaseId,
        title: title,
        artist: "Artist",
        pressing: trackCount.map {
            BridgeMatchedPressing(
                year: 2000,
                media: PreviewData.media(.cd),
                trackCount: $0
            )
        },
        cover: cover,
        evidence: BridgeMatchEvidence(source: .musicBrainz, signal: .discId)
    )
}

/// An identified Pending row: matched, no import status.
private func identifiedRow(
    _ key: String,
    title: String,
    matchedCover: BridgeRemoteImageSet? = nil,
    metadataSummary: BridgeTriageMetadataSummary? = nil,
    cover: BridgeCoverImageSource? = nil
) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        actionable: true,
        placement: .pending,
        actionBasis: BridgeCandidateActionBasis(
            actionable: true,
            placement: .pending,
            draftValid: true,
            lookup: nil,
            separable: false
        ),
        matched: matchedRelease(
            releaseId: "rel-\(key)",
            title: title,
            cover: matchedCover
        ),
        metadataSummary: metadataSummary,
        cover: cover,
        importStatus: nil,
        metadataProvenance: .externalRelease(
            record: BridgeMetadataRef(catalog: .musicBrainz, key: "rel-\(key)"),
            partners: []
        ),
        reading: metadataSummary == nil
            ? .unidentified
            : .identified(records: [
                BridgeReleaseRecord(
                    catalog: .musicBrainz,
                    url: "https://musicbrainz.org/release/rel-\(key)"
                )
            ]),
        selected: false
    )
}

private func skippedRow(_ key: String, title: String) -> BridgeTriageRow {
    BridgeTriageRow(
        candidateKey: key,
        folderName: title,
        watchedFolderPath: "/w",
        displayPath: title,
        actionable: true,
        placement: .skipped,
        actionBasis: BridgeCandidateActionBasis(
            actionable: true,
            placement: .skipped,
            draftValid: false,
            lookup: nil,
            separable: false
        ),
        matched: nil,
        metadataSummary: nil,
        cover: nil,
        importStatus: nil,
        metadataProvenance: nil,
        reading: .unidentified,
        selected: false
    )
}

private func detail(
    folderPath: String,
    watchedFolderPath: String,
    name: String,
    skipped: Bool = false,
    resumedIdentifyState: BridgeIdentifyState = .idle,
    cover: BridgeCoverChoice? = nil,
    release: BridgeReleaseDetail? = nil,
    presentation: BridgeMetadataPresentation = .draft
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
        placement: .pending(folderCheck: nil, records: []),
        live: BridgeCandidateLiveState(
            identification: nil,
            import: nil,
            actions: [
                .import, .identify, .resetToFileMetadata, .clearMetadata,
                .skip,
            ]
        ),
        importStatus: nil,
        release: release,
        pickedLibraryStatus: nil,
        fileEvidence: [],
        metadataDraft: MappingFixtures.albumEdit,
        artistResolutions: [],
        metadataDraftIsBlank: false,
        metadataProvenance: MappingFixtures.provenance,
        metadataAuthor: .person,
        metadataRevision: 1,
        mapping: BridgeMappingTable(
            images: [],
            trackSections: [],
            files: [],
            reconciliation: nil
        ),
        cover: cover,
        signals: nil,
        failure: nil,
        session: MappingFixtures.session(presentation: presentation)
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
                resumedIdentifyState: .notFoundAnywhere(run: nil)
            )
        )

        let read = try #require(store.selectedCandidates["/w1/a"])
        #expect(read.displayName == "A")
        // With no run live the resumed state is what the pane shows.
        #expect(read.resumedIdentifyState == .notFoundAnywhere(run: nil))
        #expect(read.placement == .pending(folderCheck: nil, records: []))
        #expect(read.importStatus == nil)
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
        store.selectedCandidates["/w1/a"] = existing

        // Same key, renamed and skipped, and the pane's stored session moved on.
        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A-renamed",
                skipped: true,
                presentation: .findOnline
            )
        )

        let merged = try #require(store.selectedCandidates["/w1/a"])
        // The pane's in-memory work survives the re-read.
        #expect(merged.libraryStatuses["rel-1"] != nil)
        // The pane's session is the candidate's, so it comes with the read.
        #expect(merged.metadataPresentation == .findOnline)
        // Scan fields come from the incoming read.
        #expect(merged.displayName == "A-renamed")
        #expect(merged.files.files.isEmpty)
    }

    /// A stored pick that asks nothing moves the open pane from Find online to
    /// the draft.
    @MainActor
    @Test("a read that stores the pane on the draft moves it there")
    func aReadOnTheDraftMovesThePane() throws {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                presentation: .findOnline
            )
        )
        #expect(
            try #require(store.selectedCandidates["/w1/a"])
                .metadataPresentation == .findOnline
        )

        store.applyCandidateDetail(
            key: "/w1/a",
            detail: detail(
                folderPath: "/w1/a",
                watchedFolderPath: "/w1",
                name: "A",
                presentation: .draft
            )
        )

        #expect(
            try #require(store.selectedCandidates["/w1/a"])
                .metadataPresentation == .draft
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
            let row = identifiedRow(
                key,
                title: "Subject",
                matchedCover: BridgeRemoteImageSet(
                    url: "https://example.com/queue-cover.jpg",
                    downscaled: []
                ),
                metadataSummary: BridgeTriageMetadataSummary(
                    albumTitle: "Applied Draft",
                    albumArtistAssignments: []
                ),
                cover: choice.image
            )
            store.applyCandidateDetail(
                key: key,
                detail: detail(
                    folderPath: key,
                    watchedFolderPath: "/w",
                    name: "Subject",
                    cover: choice
                )
            )
            store.selectedCandidates.removeValue(forKey: key)

            #expect(
                store.sidebarCover(for: row)
                    == ImageContent(bridge: choice.image)
            )
        }
    }

    @Test("the sidebar renders only the cover resolved by core")
    func sidebarDoesNotDeriveACoverFromMatchMetadata() {
        let row = identifiedRow(
            "/w/subject",
            title: "Subject",
            matchedCover: BridgeRemoteImageSet(
                url: "https://example.com/queue-cover.jpg",
                downscaled: []
            )
        )

        #expect(ImportStore().sidebarCover(for: row) == nil)
    }
}

// MARK: - The paged list

/// How many pages the paged list has taken delivery of.
@MainActor
private final class DeliveredPages {
    var count = 0
}

@MainActor
private final class ViewDeliveryOutcome {
    enum State: Equatable {
        case waiting
        case delivered
        case failed
    }

    var state = State.waiting
}

@Suite("Import list page source")
struct ImportListPageSourceTests {
    private struct ViewReadFailed: Error {}

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

    /// A stub bridge subscription recording the windows asked for and handing
    /// back queued values.
    private final class StubListSubscription: ImportListSubscriptionProtocol,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var windows: [[BridgeLibraryPageWindow]] = []
        private var pending: [Result<BridgeImportListSnapshot, any Error>] = []
        private var viewRevision: UInt64 = 0
        private var views: [BridgeImportListView] = []
        private var setViewHook: (() -> Void)?
        private var askedCount = 0
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

        /// How many times the source has asked for a value; an ask past a
        /// delivered value means that value was handled.
        var asks: Int {
            lock.withLock { askedCount }
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
                askedCount += 1
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
            row: identifiedRow(key, title: key),
            isGroupMember: false
        )
    }

    private func snapshot(
        _ windows: [SnapshotWindow],
        totalCount: UInt64,
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
                folderScanActivity: nil,
                groupKeys: [],
                pendingCovers: [],
                pendingFilters: []
            ),
            selectionRevision: 0,
            requestRevision: requestRevision,
            cause: .requestChanged
        )
    }
}

extension ImportListPageSourceTests {
    @MainActor
    @Test("a view waits for the revision that answers it")
    func aViewWaitsForTheRevisionThatAnswersIt() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let view = BridgeImportListView(
            tab: .pending,
            filterText: "",
            pendingFilters: [],
            collapsedGroups: [],
            order: .pathAscending
        )
        let outcome = ViewDeliveryOutcome()
        Task {
            do {
                try await source.pages.waitForView(view)
                outcome.state = .delivered
            }
            catch {
                outcome.state = .failed
            }
        }
        try await Wait.until({ subscription.requestedViews == [view] })

        // The source's second ask comes after it handled revision 0, and the yield
        // runs this test after any waiter revision 0 wrongly released.
        subscription.deliver(
            snapshot([], totalCount: 70, requestRevision: 0)
        )
        try await Wait.until { subscription.asks >= 2 }
        await Task.yield()
        #expect(outcome.state == .waiting)

        subscription.deliver(
            snapshot([], totalCount: 70, requestRevision: 1)
        )
        try await Wait.until({ outcome.state != .waiting })
        #expect(outcome.state == .delivered)
    }

    @MainActor
    @Test("a view wait's cancellation cannot miss registration")
    func viewWaitCancellationAtRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let task = Task {
            try await source.pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilters: [],
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        task.cancel()

        do {
            _ = try await task.value
            Issue.record("a cancelled view wait returned")
        }
        catch is CancellationError {}
        catch {
            Issue.record("unexpected cancellation error: \(error)")
        }
    }

    @MainActor
    @Test("a view wait's registration receives a source failure")
    func viewWaitFailureAtRegistration() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let task = Task {
            try await source.pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilters: [],
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }
        try await Wait.until({ !subscription.requestedViews.isEmpty })

        subscription.fail(ViewReadFailed())

        do {
            _ = try await task.value
            Issue.record("a failed view wait returned")
        }
        catch is ViewReadFailed {}
        catch {
            Issue.record("unexpected view wait error: \(error)")
        }
    }

    @Test("a failure between view acceptance and registration is retained")
    func viewWaitFailureBeforeRegistration() async {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let failureDelivered = DispatchSemaphore(value: 0)
        _ = source.subscribe(
            offset: 0,
            limit: 1,
            onValue: { _, _ in },
            onError: { _ in failureDelivered.signal() }
        )
        subscription.onSetView {
            subscription.fail(ViewReadFailed())
            failureDelivered.wait()
        }
        let pages = source.pages
        let task = Task {
            try await pages.waitForView(
                BridgeImportListView(
                    tab: .pending,
                    filterText: "",
                    pendingFilters: [],
                    collapsedGroups: [],
                    order: .pathAscending
                )
            )
        }

        do {
            _ = try await task.value
            Issue.record("a failed view wait returned")
        }
        catch is ViewReadFailed {}
        catch {
            Issue.record("unexpected view wait error: \(error)")
        }
    }

    @MainActor
    @Test("one value updates every registered page, and its summary")
    func oneValueUpdatesEveryPage() async throws {
        let subscription = StubListSubscription()
        var summaries: [BridgeImportQueueSummary] = []
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { summaries.append($0) },
            onSelectionRevision: { _ in }
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
                totalCount: 3
            )
        )
        try await Wait.until({ totals.count == 2 })

        #expect(first == ["candidate:/w/a", "candidate:/w/b"])
        #expect(second == ["candidate:/w/c"])
        #expect(totals == [3, 3])
        #expect(summaries.last?.counts.pending == 3)
    }

    @MainActor
    @Test("cancelling one page asks for the windows that are left")
    func cancellingOnePageShrinksTheWindows() async throws {
        let subscription = StubListSubscription()
        let source = ImportListPageSource(
            subscription: subscription,
            onSummary: { _ in },
            onSelectionRevision: { _ in }
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
            onSummary: { _ in },
            onSelectionRevision: { _ in }
        )
        let importStore = ImportStore()
        // A redelivered page is observable only in the count of pages taken.
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
        try await Wait.until({ !subscription.requestedWindows.isEmpty })
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        await initial
        try await Wait.until({ delivered.count == 1 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        // A commit that leaves this window's rows where they were.
        subscription.deliver(
            snapshot(
                [SnapshotWindow(offset: 0, limit: 50, keys: firstPage)],
                totalCount: UInt64(total)
            )
        )
        try await Wait.until({ delivered.count == 2 })
        #expect(loadedKeys(list, importStore, 0..<50) == firstPage)

        // Scrolling past the page boundary registers a second window; the first
        // window's rows stay resolvable meanwhile.
        async let next: Void = list.loadPage(containing: 55)
        try await Wait.until({ subscription.requestedWindows.last?.count == 2 })
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
        try await Wait.until({ delivered.count == 4 })
        #expect(loadedKeys(list, importStore, 0..<60) == keys)
    }

    /// The keys the list holds at `positions`, resolved as a row does.
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
            case .imported(_, let row): return row.candidateKey
            case .groupHeader, .invalid: return nil
            }
        }
    }
}
