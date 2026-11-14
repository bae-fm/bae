#if DEBUG
    import Foundation

    // Reads over fixed rows, for shots and tests: each answers every
    // request with its slice of rows given already in order, which only core
    // orders and windows in a running app. Debug builds only.

    extension PulledValues {
        /// A read that answers `value` once and then waits, for previews and
        /// tests of a live read with nothing more to say.
        public static func fixed(_ value: Value) -> PulledValues<Value> {
            let answered = FixedAnswer()
            return PulledValues(read: {
                if answered.take() { return value }
                try await Task.sleep(for: .seconds(3600))
                throw CancellationError()
            })
        }
    }

    private final class FixedAnswer: @unchecked Sendable {
        private let lock = NSLock()
        private var answered = false

        /// True the first time it is asked.
        func take() -> Bool {
            lock.withLock {
                defer { answered = true }
                return !answered
            }
        }
    }

    extension KeyedDetailQuery {
        /// A read over fixed details, for shots and tests: every id is
        /// answered with what `lookup` finds for it.
        public static func fixed(
            answers: UnansweredCount,
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

    /// The requests a fixed query answers: the latest one not yet answered,
    /// handed to the one reader waiting for it. A request counts in `answers`
    /// from when it is made until its reader asks for the next one, or the
    /// query closes; one replacing a request no reader has taken yet is the
    /// same read asked again.
    final class FixedRequests<Request: Sendable>: @unchecked Sendable {
        private let answers: UnansweredCount
        private let lock = NSLock()
        private var pending: Request?
        private var waiter: CheckedContinuation<Request, any Error>?
        /// A request handed to the reader whose answer it has not taken up.
        private var handedOut = false
        private var closed = false

        init(answers: UnansweredCount) {
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

    extension StorageSelectionQuery {
        /// A selection over fixed `rows`, for shots and tests: every set of
        /// ids is answered with the rows among `rows` it names, beside the
        /// actions all of them allow.
        public static func fixed(
            _ rows: [BridgeStorageRow],
            answers: UnansweredCount
        ) -> StorageSelectionQuery {
            let requests = FixedRequests<[String]>(answers: answers)
            requests.request([])
            return StorageSelectionQuery(
                setSelected: { requests.request($0) },
                values: PulledValues(
                    read: {
                        let ids = Set(try await requests.take())
                        let selected = rows.filter { ids.contains($0.id) }
                        return BridgeStorageSelection(
                            selected: selected,
                            // The fixture's stand-in for core's answer: the
                            // actions every selected row lists.
                            selectionActions: selected.first.map { first in
                                first.release.storageActions.filter { action in
                                    selected.allSatisfy {
                                        $0.release.storageActions.contains(
                                            action
                                        )
                                    }
                                }
                            } ?? []
                        )
                    },
                    cancel: { requests.close() }
                )
            )
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

    /// In-memory page source for album shots and tests. Expects rows to
    /// arrive pre-sorted; serves contiguous slices.
    public struct FixedAlbumPageSource: PageSource {
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

    extension BridgeListQueries {
        /// The key a fixed list gives its row at `position`: the position
        /// itself, since the rows arrive already in order.
        public static func fixedKey(_ position: Int) -> [BridgeSortValue] {
            [.integer(value: Int64(position))]
        }

        private static func fixedPosition(_ key: [BridgeSortValue]) -> Int {
            if case .integer(let value)? = key.first { return Int(value) }
            return 0
        }

        /// The delivery a fixed list answers `window` with: the rows of its
        /// range, where the keys are positions.
        static func fixedDelivery(
            of window: BridgeListWindow,
            rows: [Row],
            leadingHeader: @Sendable (_ position: Int) -> Row? = { _ in nil }
        ) -> ListWindowDelivery<Row> {
            switch window {
            case .forward(let from, let until, let limit):
                let start = min(from.map(fixedPosition) ?? 0, rows.count)
                let end = min(
                    until.map(fixedPosition) ?? rows.count,
                    rows.count
                )
                let stop = min(start + Int(limit), end)
                return ListWindowDelivery(
                    window: window,
                    rows: start < stop ? Array(rows[start..<stop]) : [],
                    first: start < stop ? fixedKey(start) : nil,
                    beyond: stop < end ? fixedKey(stop) : nil,
                    leadingHeader: start < stop ? leadingHeader(start) : nil
                )
            case .backward(let until, let limit):
                let end = min(fixedPosition(until), rows.count)
                let start = max(0, end - Int(limit))
                return ListWindowDelivery(
                    window: window,
                    rows: start < end ? Array(rows[start..<end]) : [],
                    first: start < end ? fixedKey(start) : nil,
                    beyond: start > 0 ? fixedKey(start - 1) : nil,
                    leadingHeader: start < end ? leadingHeader(start) : nil
                )
            }
        }

        /// The window read of a fixed list: each request counts in `answers`
        /// until it is answered with its range of `rows`.
        private static func fixedWindow(
            of rows: [Row],
            answers: UnansweredCount,
            leadingHeader: @escaping @Sendable (_ position: Int) -> Row?
        ) -> @Sendable (BridgeListWindow) -> ListWindowQuery<Row> {
            { first in
                let requests = FixedRequests<BridgeListWindow>(answers: answers)
                requests.request(first)
                let values = PulledValues<ListWindowDelivery<Row>>(
                    read: {
                        let asked = try await requests.take()
                        return Self.fixedDelivery(
                            of: asked,
                            rows: rows,
                            leadingHeader: leadingHeader
                        )
                    },
                    cancel: { requests.close() }
                )
                return ListWindowQuery(
                    setWindow: { requests.request($0) },
                    values: values
                )
            }
        }

        /// A list over fixed, already-sorted rows, for shots and tests: every
        /// window is answered with its range of `rows`, and a key is the
        /// position of its row. Each window request counts in `answers` until
        /// it is answered.
        public static func fixed(
            _ rows: [Row],
            answers: UnansweredCount,
            itemCount: Int? = nil,
            sections: [BridgeLibraryBrowseSection] = [],
            extra countExtra: BridgeListCountExtra? = nil,
            leadingHeader: @escaping @Sendable (_ position: Int) -> Row? = {
                _ in nil
            }
        ) -> BridgeListQueries<Row> {
            let extra: BridgeListCountExtra =
                countExtra
                ?? itemCount.map {
                    BridgeListCountExtra.albumGrid(albums: UInt64($0))
                }
                ?? BridgeListCountExtra.none
            let window = Self.fixedWindow(
                of: rows,
                answers: answers,
                leadingHeader: leadingHeader
            )
            let count: @Sendable () -> PulledValues<BridgeListCount> = {
                let value = BridgeListCount(
                    rows: UInt64(rows.count),
                    extra: extra
                )
                return PulledValues<BridgeListCount>.fixed(value)
            }
            let position: @Sendable ([BridgeSortValue]) -> ListPositionQuery = {
                key in
                let value = BridgeListPosition(
                    key: key,
                    rowsBefore: UInt64(Self.fixedPosition(key))
                )
                return ListPositionQuery(
                    setKey: { _ in },
                    values: PulledValues<BridgeListPosition>.fixed(value)
                )
            }
            let keyAt: @Sendable (UInt64) async throws -> [BridgeSortValue]? = {
                position in
                if rows.isEmpty { return nil }
                return Self.fixedKey(min(Int(position), rows.count - 1))
            }
            var headings:
                (@Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>)?
            if !sections.isEmpty {
                headings = {
                    PulledValues<[BridgeLibraryBrowseSection]>.fixed(sections)
                }
            }
            return BridgeListQueries<Row>(
                window: window,
                count: count,
                position: position,
                keyAt: keyAt,
                sections: headings
            )
        }

        /// A list whose every window read fails with `error`, for shots and
        /// tests of the screen that shows a list that could not be read.
        public static func failing(
            _ error: any Error & Sendable,
            answers: UnansweredCount
        ) -> BridgeListQueries<Row> {
            let empty = BridgeListQueries<Row>.fixed([], answers: answers)
            return BridgeListQueries<Row>(
                window: { first -> ListWindowQuery<Row> in
                    let requests = FixedRequests<BridgeListWindow>(
                        answers: answers
                    )
                    requests.request(first)
                    return ListWindowQuery(
                        setWindow: { requests.request($0) },
                        values: PulledValues<ListWindowDelivery<Row>>(
                            read: {
                                _ = try await requests.take()
                                throw error
                            },
                            cancel: { requests.close() }
                        )
                    )
                },
                count: empty.count,
                position: empty.position,
                keyAt: empty.keyAt
            )
        }
    }

    #if os(macOS)
        extension ImportListHandle {
            /// An import list over fixed, already-ordered `items`, for shots and
            /// tests: every window is answered with its range of them, a view
            /// or fold changes nothing, and a candidate is found where `items`
            /// holds it, on `tab`.
            public static func fixed(
                items: [BridgeImportListItem],
                tab: BridgeTriageTab,
                answers: UnansweredCount = UnansweredCount(),
                summary: BridgeImportQueueSummary? = nil
            ) -> ImportListHandle {
                let queries = BridgeListQueries<BridgeImportListItem>
                    .fixed(
                        items,
                        answers: answers,
                        extra: BridgeListCountExtra.import(narrowed: nil)
                    )
                return ImportListHandle(
                    queries: queries,
                    summary: {
                        PulledValues<BridgeImportQueueSummary>
                            .fixed(
                                summary
                                    ?? BridgeImportQueueSummary(
                                        watchedFolders: [],
                                        hasGroups: false,
                                        narrowing: BridgeImportListNarrowing(
                                            tab: tab,
                                            filterText: "",
                                            pendingFilter: .all
                                        )
                                    )
                            )
                    },
                    filterEntries: {
                        PulledValues<[BridgePendingFilterEntry]>.fixed([])
                    },
                    selection: {
                        PulledValues<BridgeSelectionSummary>
                            .fixed(
                                BridgeSelectionSummary(
                                    count: 0,
                                    single: nil,
                                    offers: []
                                )
                            )
                    },
                    setView: { _ in nil },
                    setGroupFolded: { _, _ in },
                    setAllGroupsFolded: { _ in },
                    locateCandidate: { locate($0, in: items, on: tab) },
                    firstIdentifyingCandidate: { nil }
                )
            }

            /// Where the candidate `key` sits in the fixed `items`, on `tab`.
            private static func locate(
                _ key: Data,
                in items: [BridgeImportListItem],
                on tab: BridgeTriageTab
            ) -> BridgeImportCandidateListLocation? {
                items.firstIndex { item in
                    switch item {
                    case .candidate(_, let row, _):
                        row.candidateKey == key
                    case .invalid(_, let invalid, _):
                        invalid.candidateKey == key
                    case .groupHeader:
                        false
                    }
                }
                .map {
                    BridgeImportCandidateListLocation(
                        stableKey: items[$0].id,
                        tab: tab,
                        groupKey: nil,
                        visiblePosition: UInt64($0),
                        key: BridgeListQueries<BridgeImportListItem>
                            .fixedKey($0)
                    )
                }
            }
        }
    #endif

    extension AlbumList {
        /// A pre-populated list for shots and tests, seeded with
        /// `albums` so the view renders without waiting for an async
        /// round-trip.
        ///
        /// Rows render in the order given. The live grid is ordered by the database,
        /// through the `ORDER BY` its window queries build; a shot holds a hand-written
        /// list with no database, so it presents the order its author wrote.
        public static func preview(albums: [BridgeAlbum]) -> AlbumList {
            let list = AlbumList(
                queries: .fixed(albums, answers: UnansweredCount()),
                onError: { _ in },
            )
            list.preloadRows(rows: albums)
            return list
        }
    }

    extension ReleaseFilesQuery {
        /// A read over fixed files, for shots and tests: every request is
        /// answered with `files` as given, whatever order it asks for.
        public static func fixed(
            _ files: [BridgeFile],
            answers: UnansweredCount
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
