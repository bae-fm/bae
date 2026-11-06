#if DEBUG
    import Foundation
    import Observation

    // Reads over fixed rows, for previews and tests: each answers every
    // request with its slice of rows given already in order, which only core
    // orders and windows in a running app. Debug builds only.

    extension KeyedDetailQuery {
        /// A read over fixed details, for previews and tests: every id is
        /// answered with what `lookup` finds for it.
        public static func fixed(
            answers: FixedAnswers,
            _ lookup: @escaping @Sendable (Id) -> Value?
        ) -> KeyedDetailQuery<Value, Id> {
            let requests = FixedRequests<Id>(answers: answers)
            return KeyedDetailQuery(
                setId: { requests.request($0) },
                values: PulledValues(
                    read: {
                        let id = try await requests.take()
                        return KeyedDetailDelivery(id: id, value: lookup(id))
                    },
                    cancel: { requests.close() }
                )
            )
        }
    }

    extension LibraryBrowseQuery {
        /// A query over a fixed, already-sorted list, for previews and tests:
        /// every window request is answered with its slice.
        public static func fixed(
            _ rows: [Row],
            answers: FixedAnswers
        ) -> LibraryBrowseQuery<Row> {
            let requests = FixedRequests<[BridgeLibraryPageWindow]>(
                answers: answers
            )
            return LibraryBrowseQuery(
                setWindows: { requests.request($0) },
                values: PulledValues(
                    read: {
                        let windows = try await requests.take()
                        return LibraryBrowseDelivery(
                            windows: windows.map { window in
                                let start = min(Int(window.offset), rows.count)
                                let end = min(
                                    start + Int(window.limit),
                                    rows.count
                                )
                                return .init(
                                    window: window,
                                    rows: Array(rows[start..<end])
                                )
                            },
                            totalCount: rows.count
                        )
                    },
                    cancel: { requests.close() }
                )
            )
        }
    }

    /// How many reads a fixture's fixed queries were asked and have not yet
    /// answered, for a test that waits until a hosted view has what it asked
    /// for. A fixed query answers off the main thread; an answer counts once
    /// the screen reading it has taken it up and asked for its next value.
    public final class FixedAnswers: Observable, @unchecked Sendable {
        private let registrar = ObservationRegistrar()
        private let lock = NSLock()
        private var count = 0

        public init() {}

        /// Reads asked and not yet answered.
        public var unanswered: Int {
            registrar.access(self, keyPath: \.unanswered)
            return lock.withLock { count }
        }

        fileprivate func change(by delta: Int) {
            guard delta != 0 else { return }
            registrar.withMutation(of: self, keyPath: \.unanswered) {
                lock.withLock { count += delta }
            }
        }
    }

    /// The requests a fixed query answers: the latest one not yet answered,
    /// handed to the one reader waiting for it. A request counts in `answers`
    /// from when it is made until its reader asks for the next one, or the
    /// query closes; one replacing a request no reader has taken yet is the
    /// same read asked again.
    final class FixedRequests<Request: Sendable>: @unchecked Sendable {
        private let answers: FixedAnswers
        private let lock = NSLock()
        private var pending: Request?
        private var waiter: CheckedContinuation<Request, any Error>?
        /// A request handed to the reader whose answer it has not taken up.
        private var handedOut = false
        private var closed = false

        init(answers: FixedAnswers) {
            self.answers = answers
        }

        func request(_ request: Request) {
            let (waiter, asked):
                (CheckedContinuation<Request, any Error>?, Int) =
                    lock.withLock {
                        // A closed query answers nothing more.
                        if closed { return (nil, 0) }
                        if let waiter = self.waiter {
                            self.waiter = nil
                            handedOut = true
                            return (waiter, 1)
                        }
                        let asked = pending == nil ? 1 : 0
                        pending = request
                        return (nil, asked)
                    }
            answers.change(by: asked)
            waiter?.resume(returning: request)
        }

        /// The next request, once there is one. Asking again means the
        /// reader took up the answer to the last.
        func take() async throws -> Request {
            let answered = lock.withLock {
                defer { handedOut = false }
                return handedOut ? 1 : 0
            }
            answers.change(by: -answered)
            return try await withCheckedThrowingContinuation { continuation in
                let result: Result<Request, any Error>? = lock.withLock {
                    if closed { return .failure(CancellationError()) }
                    if let pending {
                        self.pending = nil
                        handedOut = true
                        return .success(pending)
                    }
                    waiter = continuation
                    return nil
                }
                if let result { continuation.resume(with: result) }
            }
        }

        func close() {
            let (waiter, open):
                (CheckedContinuation<Request, any Error>?, Int) =
                    lock.withLock {
                        closed = true
                        let waiter = self.waiter
                        self.waiter = nil
                        let open =
                            (handedOut ? 1 : 0) + (pending == nil ? 0 : 1)
                        handedOut = false
                        pending = nil
                        return (waiter, open)
                    }
            answers.change(by: -open)
            waiter?.resume(throwing: CancellationError())
        }
    }

    extension StorageBrowseQuery {
        /// A query over a fixed, already-sorted list, for previews and tests:
        /// every view is answered with its windows' slices of `rows`, whatever
        /// the sort, filter and search, beside `queues`.
        public static func fixed(
            _ rows: [BridgeStorageRow],
            queues: BridgeStorageQueues,
            answers: FixedAnswers
        ) -> StorageBrowseQuery {
            let requests = FixedRequests<StorageBrowseView>(answers: answers)
            return StorageBrowseQuery(
                setView: { requests.request($0) },
                values: PulledValues(
                    read: {
                        snapshot(
                            for: try await requests.take(),
                            rows: rows,
                            queues: queues
                        )
                    },
                    cancel: { requests.close() }
                )
            )
        }

        /// What the fixture answers `view` with: its windows' slices of
        /// `rows`, beside `queues`.
        private static func snapshot(
            for view: StorageBrowseView,
            rows: [BridgeStorageRow],
            queues: BridgeStorageQueues
        ) -> BridgeStorageBrowseSnapshot {
            let selected = rows.filter { view.selected.contains($0.id) }
            return BridgeStorageBrowseSnapshot(
                sort: view.sort,
                filter: view.filter,
                search: view.search,
                // The fixture's stand-in for core's answer: any text narrows
                // the list.
                searching: !view.search.isEmpty,
                windows: view.windows.map { window in
                    let start = min(Int(window.offset), rows.count)
                    let end = min(start + Int(window.limit), rows.count)
                    return BridgeStorageBrowseWindow(
                        window: window,
                        rows: Array(rows[start..<end])
                    )
                },
                totalCount: UInt64(rows.count),
                totalSize: UInt64(
                    rows.reduce(Int64(0)) { $0 + $1.release.totalSize }
                ),
                selected: selected,
                // The fixture's stand-in for core's answer: the actions every
                // selected row lists.
                selectionActions: selected.first.map { first in
                    first.release.storageActions.filter { action in
                        selected.allSatisfy {
                            $0.release.storageActions.contains(action)
                        }
                    }
                } ?? [],
                queues: queues
            )
        }
    }

    extension ArtistSearchQuery {
        /// A search over a fixed, already-ordered list, for previews and tests:
        /// every view is answered with its windows' slices of `results`, whatever
        /// the text.
        public static func fixed(
            _ results: [BridgeArtistSearchResult],
            answers: FixedAnswers
        ) -> ArtistSearchQuery {
            let requests = FixedRequests<(UInt64, [BridgeLibraryPageWindow])>(
                answers: answers
            )
            let revisions = FixedRevisions()
            return ArtistSearchQuery(
                setView: { _, windows in
                    let revision = revisions.advance()
                    requests.request((revision, windows))
                    return revision
                },
                values: PulledValues(
                    read: {
                        let (revision, windows) = try await requests.take()
                        return BridgeArtistSearchSnapshot(
                            windows: windows.map { window in
                                let start = min(
                                    Int(window.offset),
                                    results.count
                                )
                                let end = min(
                                    start + Int(window.limit),
                                    results.count
                                )
                                return BridgeArtistSearchWindow(
                                    window: window,
                                    rows: Array(results[start..<end])
                                )
                            },
                            totalCount: UInt64(results.count),
                            requestRevision: revision
                        )
                    },
                    cancel: { requests.close() }
                )
            )
        }
    }

    /// The revisions a fixed search hands out, one per view.
    private final class FixedRevisions: @unchecked Sendable {
        private let lock = NSLock()
        private var revision: UInt64 = 0

        func advance() -> UInt64 {
            lock.withLock {
                revision += 1
                return revision
            }
        }
    }

    private final class TaskPageSubscription: PageSubscription,
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

    /// In-memory page source for album previews and tests. Expects rows to
    /// arrive pre-sorted; serves contiguous slices.
    public struct AlbumPreviewPageSource: PageSource {
        public let albums: [BridgeAlbum]

        public init(albums: [BridgeAlbum]) {
            self.albums = albums
        }

        public func subscribe(
            offset: Int,
            limit: Int,
            onValue:
                @escaping @MainActor @Sendable ([BridgeAlbum], Int) -> Void,
            onError _: @escaping @MainActor @Sendable (any Error) -> Void
        ) -> any PageSubscription {
            let albums = albums
            return TaskPageSubscription(
                Task { @MainActor in
                    let start = min(offset, albums.count)
                    let end = min(start + limit, albums.count)
                    onValue(Array(albums[start..<end]), albums.count)
                }
            )
        }
    }

    extension AlbumList {
        /// A pre-populated list for SwiftUI previews and tests, seeded with
        /// `albums` so the preview renders without waiting for an async
        /// round-trip.
        ///
        /// Rows render in the order given. The live grid is ordered by the database,
        /// through the `ORDER BY` its page query builds; a preview holds a hand-written
        /// list with no database, so it presents the order its author wrote.
        public static func preview(albums: [BridgeAlbum]) -> AlbumList {
            let list = AlbumList(
                pageSource: AlbumPreviewPageSource(albums: albums),
                onError: { _ in },
            )
            list.preloadForPreview(rows: albums)
            return list
        }
    }

    extension ReleaseFilesQuery {
        /// A read over fixed files, for previews and tests: every request is
        /// answered with `files` as given, whatever order it asks for.
        public static func fixed(
            _ files: [BridgeFile],
            answers: FixedAnswers
        ) -> ReleaseFilesQuery {
            let requests = FixedRequests<BridgeReleaseFilesRequest?>(
                answers: answers
            )
            return ReleaseFilesQuery(
                setRequest: { requests.request($0) },
                values: PulledValues(
                    read: {
                        BridgeReleaseFilesSnapshot(
                            request: try await requests.take(),
                            files: files
                        )
                    },
                    cancel: { requests.close() }
                )
            )
        }
    }
#endif
