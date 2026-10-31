#if DEBUG
    import Foundation

    // Reads over fixed rows, for previews and tests: each answers every
    // request with its slice of rows given already in order, which only core
    // orders and windows in a running app. Debug builds only.

    extension DetailQuery {
        /// A read over fixed details, for previews and tests: every id is
        /// answered with what `lookup` finds for it.
        public static func fixed(
            _ lookup: @escaping @Sendable (String) -> Value?
        ) -> DetailQuery<Value> {
            let requests = FixedRequests<String?>()
            return DetailQuery(
                setId: { requests.request($0) },
                next: {
                    let id = try await requests.next()
                    return DetailDelivery(id: id, value: id.flatMap(lookup))
                },
                cancel: { requests.close() }
            )
        }
    }

    extension LibraryBrowseQuery {
        /// A query over a fixed, already-sorted list, for previews and tests:
        /// every window request is answered with its slice.
        public static func fixed(_ rows: [Row]) -> LibraryBrowseQuery<Row> {
            let requests = FixedRequests<[BridgeLibraryPageWindow]>()
            return LibraryBrowseQuery(
                setWindows: { requests.request($0) },
                next: {
                    let windows = try await requests.next()
                    return LibraryBrowseDelivery(
                        windows: windows.map { window in
                            let start = min(Int(window.offset), rows.count)
                            let end = min(start + Int(window.limit), rows.count)
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
        }
    }

    /// The requests a fixed query answers: the latest one not yet answered,
    /// handed to the one reader waiting for it.
    final class FixedRequests<Request: Sendable>: @unchecked Sendable {
        private let lock = NSLock()
        private var pending: Request?
        private var waiter: CheckedContinuation<Request, any Error>?
        private var closed = false

        func request(_ request: Request) {
            let waiter: CheckedContinuation<Request, any Error>? = lock.withLock
            {
                if let waiter = self.waiter {
                    self.waiter = nil
                    return waiter
                }
                pending = request
                return nil
            }
            waiter?.resume(returning: request)
        }

        func next() async throws -> Request {
            try await withCheckedThrowingContinuation { continuation in
                let result: Result<Request, any Error>? = lock.withLock {
                    if closed { return .failure(CancellationError()) }
                    if let pending {
                        self.pending = nil
                        return .success(pending)
                    }
                    waiter = continuation
                    return nil
                }
                if let result { continuation.resume(with: result) }
            }
        }

        func close() {
            let waiter: CheckedContinuation<Request, any Error>? = lock.withLock
            {
                closed = true
                let waiter = self.waiter
                self.waiter = nil
                return waiter
            }
            waiter?.resume(throwing: CancellationError())
        }
    }

    extension StorageBrowseQuery {
        /// A query over a fixed, already-sorted list, for previews and tests:
        /// every view is answered with its windows' slices of `rows`, whatever
        /// the sort, filter and search, beside `queues`.
        public static func fixed(
            _ rows: [BridgeStorageRow],
            queues: BridgeStorageQueues
        ) -> StorageBrowseQuery {
            let requests = FixedRequests<StorageBrowseView>()
            return StorageBrowseQuery(
                setView: { requests.request($0) },
                next: {
                    let view = try await requests.next()
                    let selected = rows.filter { view.selected.contains($0.id) }
                    return BridgeStorageBrowseSnapshot(
                        sort: view.sort,
                        filter: view.filter,
                        search: view.search,
                        // The fixture's stand-in for core's answer: any text
                        // narrows the list.
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
                        // The fixture's stand-in for core's answer: the actions
                        // every selected row lists.
                        selectionActions: selected.first.map { first in
                            first.release.storageActions.filter { action in
                                selected.allSatisfy {
                                    $0.release.storageActions.contains(action)
                                }
                            }
                        } ?? [],
                        queues: queues
                    )
                },
                cancel: { requests.close() }
            )
        }
    }

    extension ArtistSearchQuery {
        /// A search over a fixed, already-ordered list, for previews and tests:
        /// every view is answered with its windows' slices of `results`, whatever
        /// the text.
        public static func fixed(_ results: [BridgeArtistSearchResult])
            -> ArtistSearchQuery
        {
            let requests = FixedRequests<(UInt64, [BridgeLibraryPageWindow])>()
            let revisions = FixedRevisions()
            return ArtistSearchQuery(
                setView: { _, windows in
                    let revision = revisions.advance()
                    requests.request((revision, windows))
                    return revision
                },
                next: {
                    let (revision, windows) = try await requests.next()
                    return BridgeArtistSearchSnapshot(
                        windows: windows.map { window in
                            let start = min(Int(window.offset), results.count)
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
        public static func fixed(_ files: [BridgeFile]) -> ReleaseFilesQuery {
            let requests = FixedRequests<BridgeReleaseFilesRequest?>()
            return ReleaseFilesQuery(
                setRequest: { requests.request($0) },
                next: {
                    BridgeReleaseFilesSnapshot(
                        request: try await requests.next(),
                        files: files
                    )
                },
                cancel: { requests.close() }
            )
        }
    }
#endif
