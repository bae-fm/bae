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
        /// A selection read whose answers the caller chose, in core's place,
        /// for shots and tests: the fixture answers each set of ids with what
        /// `answer` returns for it.
        public static func fixed(
            answers: UnansweredCount,
            _ answer:
                @escaping @Sendable (Set<String>) -> BridgeStorageSelection
        ) -> StorageSelectionQuery {
            let requests = FixedRequests<[String]>(answers: answers)
            requests.request([])
            return StorageSelectionQuery(
                setSelected: { requests.request($0) },
                values: PulledValues(
                    read: { answer(Set(try await requests.take())) },
                    cancel: { requests.close() }
                )
            )
        }
    }

    extension BridgeListQueries {
        /// The delivery a fixed list's view answers the rows from `first` to
        /// `last` on screen with: those rows and a screen of rows on each
        /// side, as core sizes a window, where the rows arrive in order.
        public static func fixedDelivery(
            first: Int,
            last: Int,
            rows: [Row]
        ) -> ListViewDelivery<Row> {
            let screen = last - first + 1
            let start = min(max(0, first - screen), rows.count)
            let end = min(start + 3 * screen, rows.count)
            return ListViewDelivery(
                start: start,
                rows: start < end ? Array(rows[start..<end]) : []
            )
        }

        /// The view read of a fixed list: each report of the rows on screen
        /// counts in `answers` until the fixed list answers it with its rows
        /// around them.
        private static func fixedView(
            of rows: [Row],
            answers: UnansweredCount
        ) -> @Sendable (UInt64, UInt64) -> ListViewQuery<Row> {
            { first, last in
                let requests = FixedRequests<ClosedRange<Int>>(answers: answers)
                requests.request(Int(first)...Int(last))
                let values = PulledValues<ListViewDelivery<Row>>(
                    read: {
                        let shown = try await requests.take()
                        return Self.fixedDelivery(
                            first: shown.lowerBound,
                            last: shown.upperBound,
                            rows: rows
                        )
                    },
                    cancel: { requests.close() }
                )
                return ListViewQuery(
                    setVisible: { requests.request(Int($0)...Int($1)) },
                    values: values
                )
            }
        }

        /// A list over fixed, already-sorted rows, for shots and tests: the
        /// list's view answers the rows on screen with its rows around them.
        /// Each report of the rows on screen counts in `answers` until the
        /// list answers it.
        public static func fixed(
            _ rows: [Row],
            answers: UnansweredCount,
            sections: [BridgeLibraryBrowseSection] = [],
            extra: BridgeListCountExtra = .none
        ) -> BridgeListQueries<Row> {
            let view = Self.fixedView(of: rows, answers: answers)
            let count: @Sendable () -> PulledValues<BridgeListCount> = {
                let value = BridgeListCount(
                    rows: UInt64(rows.count),
                    items: UInt64(rows.count),
                    extra: extra
                )
                return PulledValues<BridgeListCount>.fixed(value)
            }
            var headings:
                (@Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>)?
            if !sections.isEmpty {
                headings = {
                    PulledValues<[BridgeLibraryBrowseSection]>.fixed(sections)
                }
            }
            return BridgeListQueries<Row>(
                view: view,
                count: count,
                sections: headings
            )
        }

        /// A list whose every view read fails with `error`, for shots and
        /// tests of the screen that shows a list bae failed to read.
        public static func failing(
            _ error: any Error & Sendable,
            answers: UnansweredCount
        ) -> BridgeListQueries<Row> {
            let empty = BridgeListQueries<Row>.fixed([], answers: answers)
            return BridgeListQueries<Row>(
                view: { first, last -> ListViewQuery<Row> in
                    let requests = FixedRequests<ClosedRange<Int>>(
                        answers: answers
                    )
                    requests.request(Int(first)...Int(last))
                    return ListViewQuery(
                        setVisible: { requests.request(Int($0)...Int($1)) },
                        values: PulledValues<ListViewDelivery<Row>>(
                            read: {
                                _ = try await requests.take()
                                throw error
                            },
                            cancel: { requests.close() }
                        )
                    )
                },
                count: empty.count
            )
        }
    }

    #if os(macOS)
        extension ImportListHandle {
            /// An import list over fixed, already-ordered `items`, for shots and
            /// tests, on `tab`: the list's view answers the rows on screen with
            /// the items around them under every request, a view or fold opens
            /// at the top and hides nothing, and the list finds a candidate at
            /// its place in `items`.
            public static func fixed(
                items: [BridgeImportListItem],
                tab: BridgeTriageTab,
                extra: BridgeListCountExtra = .import(
                    narrowed: nil,
                    tabs: BridgeTriageTabCounts(pending: 0, done: 0, skipped: 0)
                )
            ) -> ImportListHandle {
                let queries = BridgeListQueries<BridgeImportListItem>
                    .fixed(items, answers: UnansweredCount(), extra: extra)
                return ImportListHandle(
                    queries: { _ in queries },
                    summary: {
                        PulledValues<BridgeImportQueueSummary>
                            .fixed(
                                BridgeImportQueueSummary(
                                    hasWatchedFolders: false,
                                    hasGroups: false
                                )
                            )
                    },
                    filterEntries: {
                        PulledValues<[BridgePendingFilterEntry]>.fixed([])
                    },
                    selection: {
                        ImportSelectionQuery(
                            setSelected: { _ in },
                            values: PulledValues<BridgeSelectionSummary>
                                .fixed(
                                    BridgeSelectionSummary(
                                        count: 0,
                                        single: nil,
                                        offers: []
                                    )
                                )
                        )
                    },
                    viewChange: { _, _, _ in
                        BridgeImportListViewChange(opening: .top, left: [])
                    },
                    revealCandidate: { request, key in
                        position(of: key, in: items)
                            .map { position in
                                BridgeImportListReveal.fixed(
                                    from: request,
                                    on: tab,
                                    at: UInt64(position)
                                )
                            }
                    },
                    candidatePosition: { _, key in
                        position(of: key, in: items).map(UInt64.init)
                    },
                    firstIdentifyingCandidate: { _ in nil }
                )
            }

            /// Where the candidate `key` sits in the fixed `items`.
            private static func position(
                of key: Data,
                in items: [BridgeImportListItem]
            ) -> Int? {
                items.firstIndex { item in
                    switch item {
                    case .candidate(_, let row, _):
                        row.candidateKey == key
                    case .invalid(_, let invalid):
                        invalid.candidateKey == key
                    case .groupHeader:
                        false
                    }
                }
            }
        }

        extension BridgeImportListReveal {
            /// What core answers when asked to reveal a candidate that sits in no
            /// group from `request`: the request on `tab` with the text and
            /// Found's filter cleared, and the candidate at `position`. A fixed
            /// list or a test stands in for core with it.
            public static func fixed(
                from request: BridgeImportListRequest,
                on tab: BridgeTriageTab,
                at position: UInt64
            ) -> BridgeImportListReveal {
                var revealed = request
                revealed.view.tab = tab
                revealed.view.filterText = ""
                revealed.view.pendingFilter = .all
                return BridgeImportListReveal(
                    request: revealed,
                    position: position
                )
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
            list.preloadRows(rows: albums, extra: .none)
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
