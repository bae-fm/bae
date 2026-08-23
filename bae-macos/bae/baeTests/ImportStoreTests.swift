import BaeKit
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
    coverThumbnailUrl: String? = nil
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
        selectable: true,
        importStatus: nil,
        picked: nil,
        claim: nil
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
        selectable: false,
        importStatus: nil,
        picked: nil,
        claim: nil
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
        selectable: false,
        importStatus: .complete(releaseId: "rel-\(key)", albumId: "al-\(key)"),
        picked: nil,
        claim: nil
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
        selectable: false,
        importStatus: nil,
        picked: nil,
        claim: nil
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
        evidence: nil,
        edit: nil,
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
        existing.pickInFlight = true
        existing.search.showManualSearch = true
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
        #expect(merged.pickInFlight)
        #expect(merged.search.showManualSearch)
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

@Suite("Import loudness progress")
struct ImportLoudnessProgressTests {
    @MainActor
    @Test("preserves unavailable fractions as indeterminate")
    func preservesIndeterminateProgress() throws {
        let store = ImportStore()
        let handler = DesktopEventHandler(
            importStore: store,
            mediaControlService: MediaControlService()
        )

        handler.apply(
            .candidateImportLoudnessProgress(
                key: "candidate",
                tracksDone: 1,
                tracksTotal: 3,
                fraction: nil
            )
        )

        let event = try #require(store.importLoudnessSubject.value)
        #expect(event.key == "candidate")
        #expect(event.tracksDone == 1)
        #expect(event.tracksTotal == 3)
        #expect(event.fraction == nil)
    }
}

@Suite("ImportStore sidebar covers")
struct ImportStoreSidebarCoverTests {
    @MainActor
    @Test("the sidebar uses every cover core answers with")
    func selectedCoversWinOverQueueThumbnail() throws {
        let store = ImportStore()
        let key = "/w/subject"
        let row = readyRow(
            key,
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )

        let remoteCover = try #require(PreviewData.remoteCovers.last)
        let localArtwork = try #require(
            PreviewData.bridgeCandidateFiles.images.last
        )
        let choices = [
            remoteCover.coverChoice,
            try #require(localArtwork.coverChoice),
        ]
        for choice in choices {
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

            #expect(
                store.sidebarCover(for: row)
                    == ImageContent(bridge: choice.thumbnailSource)
            )
        }
    }

    @MainActor
    @Test("the sidebar uses the picked release's default cover")
    func defaultCoverWinsOverQueueThumbnail() throws {
        let store = ImportStore()
        let key = "/w/subject"
        let releaseDetail = PreviewData.releaseDetailBridge
        let defaultCover = try #require(releaseDetail.defaultCover)
        let row = readyRow(
            key,
            title: "Subject",
            coverThumbnailUrl: "https://example.com/queue-thumbnail.jpg"
        )
        store.applyCandidateDetail(
            key: key,
            detail: detail(
                folderPath: key,
                watchedFolderPath: "/w",
                name: "Subject",
                row: row,
                cover: defaultCover,
                release: releaseDetail
            )
        )

        #expect(
            store.sidebarCover(for: row)
                == ImageContent(bridge: defaultCover.thumbnailSource)
        )
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

@Suite("Import list page source")
struct ImportListPageSourceTests {
    /// A stub bridge subscription: it records the windows asked for and hands
    /// back the values a test queues.
    private final class StubListSubscription: ImportListSubscriptionProtocol,
        @unchecked Sendable
    {
        private let lock = NSLock()
        private var windows: [[BridgeLibraryPageWindow]] = []
        private var pending: [BridgeImportListSnapshot] = []
        private var waiter:
            CheckedContinuation<
                BridgeImportListSnapshot, Never
            >?

        var requestedWindows: [[BridgeLibraryPageWindow]] {
            lock.lock()
            defer { lock.unlock() }
            return windows
        }

        func setWindows(windows: [BridgeLibraryPageWindow]) throws {
            lock.lock()
            self.windows.append(windows)
            lock.unlock()
        }

        func setView(view: BridgeImportListView) throws {}

        func cancel() async throws {}

        func deliver(_ snapshot: BridgeImportListSnapshot) {
            lock.lock()
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { pending.append(snapshot) }
            lock.unlock()
            waiter?.resume(returning: snapshot)
        }

        func next() async -> BridgeImportListSnapshot {
            await withCheckedContinuation { continuation in
                lock.lock()
                if pending.isEmpty {
                    waiter = continuation
                    lock.unlock()
                }
                else {
                    let snapshot = pending.removeFirst()
                    lock.unlock()
                    continuation.resume(returning: snapshot)
                }
            }
        }
    }

    private func item(_ key: String) -> BridgeImportListItem {
        .candidate(
            stableKey: "candidate:\(key)",
            row: readyRow(key, title: key)
        )
    }

    private func snapshot(
        _ windows: [(UInt64, UInt64, [String])],
        totalCount: UInt64,
        firstUnidentifiedKey: String? = nil
    ) -> BridgeImportListSnapshot {
        BridgeImportListSnapshot(
            windows: windows.map { offset, limit, keys in
                BridgeImportListWindow(
                    window: BridgeLibraryPageWindow(
                        offset: offset,
                        limit: limit
                    ),
                    items: keys.map(item)
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
                firstUnidentifiedKey: firstUnidentifiedKey
            ),
            requestRevision: 0,
            cause: .requestChanged
        )
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
                [(0, 2, ["/w/a", "/w/b"]), (2, 2, ["/w/c"])],
                totalCount: 3,
                firstUnidentifiedKey: "/w/c"
            )
        )
        try await settle()

        #expect(first == ["candidate:/w/a", "candidate:/w/b"])
        #expect(second == ["candidate:/w/c"])
        #expect(totals == [3, 3])
        #expect(summaries.last?.firstUnidentifiedKey == "/w/c")
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

    /// Let the source's delivery task reach the main actor.
    private func settle() async throws {
        for _ in 0..<10 {
            await Task.yield()
        }
    }
}

@Suite("Import selection")
struct ImportSelectionTests {
    @MainActor
    @Test("a read that says the folder is gone drops it from the selection")
    func aMissingCandidateClearsItsSelection() {
        let uiStore = UiStore()
        var reported: [Set<String>] = []
        uiStore.onFolderCandidateSelectionChanged = { reported.append($0) }

        uiStore.setFolderCandidateSelection(["/w/a", "/w/b"])
        // What the per-key read does when it delivers no candidate.
        uiStore.removeFolderCandidateSelection(["/w/a"])

        #expect(uiStore.selectedFolderCandidates == ["/w/b"])
        #expect(reported == [["/w/a", "/w/b"], ["/w/b"]])
    }
}
