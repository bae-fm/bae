import Foundation

// The import list is a desktop surface: bae-bridge compiles the import list's
// types only for desktop targets, so the iOS bindings do not have them.
#if os(macOS)

    /// One item of the import list, addressed by the key core computed for it.
    extension BridgeImportListItem: Identifiable {
        public var id: String {
            switch self {
            case .groupHeader(let header):
                return header.stableKey
            case .candidate(let stableKey, _, _),
                .invalid(let stableKey, _):
                return stableKey
            }
        }
    }

    extension BridgeListRows {
        /// The items of an import list's window.
        fileprivate var importItems: [BridgeImportListItem] {
            guard case .import(let items) = self else {
                preconditionFailure(
                    "the import list delivered rows of another list"
                )
            }
            return items
        }
    }

    /// The import list: its view and count as the generic list reads them
    /// under a request, the reads of what surrounds the list, and the reads
    /// that answer what a view or a fold changes. Each call that reads rows
    /// takes the request the screen shows, which holds the view and the
    /// folded groups.
    public struct ImportListHandle: Sendable {
        /// The view and count of the rows the list `request` shows.
        public let queries:
            @Sendable (BridgeImportListRequest) -> BridgeListQueries<
                BridgeImportListItem
            >
        /// The watched folders and whether Found has groups.
        public let summary:
            @Sendable () -> PulledValues<BridgeImportQueueSummary>
        /// Found's filter entries with how many rows each holds.
        public let filterEntries:
            @Sendable () -> PulledValues<[BridgePendingFilterEntry]>
        /// A live query of the selected rows: what they hold and the actions
        /// the person can run on them. The screen sets the selected keys on
        /// the query, and the query reads the rows at those keys.
        public let selection: @Sendable () -> ImportSelectionQuery
        private let change:
            @Sendable (
                BridgeImportListRequest, BridgeImportListRequest, [Data]
            ) async throws -> BridgeImportListViewChange
        private let reveal:
            @Sendable (BridgeImportListRequest, Data) async throws
                -> BridgeImportListReveal?
        private let position:
            @Sendable (BridgeImportListRequest, Data) async throws -> UInt64?
        private let firstIdentifying:
            @Sendable (BridgeImportListOrder) async throws -> Data?

        public init(
            queries:
                @escaping @Sendable (BridgeImportListRequest) ->
                BridgeListQueries<BridgeImportListItem>,
            summary:
                @escaping @Sendable () -> PulledValues<BridgeImportQueueSummary>,
            filterEntries:
                @escaping @Sendable () -> PulledValues<
                    [BridgePendingFilterEntry]
                >,
            selection: @escaping @Sendable () -> ImportSelectionQuery,
            viewChange:
                @escaping @Sendable (
                    BridgeImportListRequest, BridgeImportListRequest, [Data]
                ) async throws -> BridgeImportListViewChange,
            revealCandidate:
                @escaping @Sendable (BridgeImportListRequest, Data)
                async throws -> BridgeImportListReveal?,
            candidatePosition:
                @escaping @Sendable (BridgeImportListRequest, Data) async throws
                -> UInt64?,
            firstIdentifyingCandidate:
                @escaping @Sendable (BridgeImportListOrder) async throws
                -> Data?
        ) {
            self.queries = queries
            self.summary = summary
            self.filterEntries = filterEntries
            self.selection = selection
            change = viewChange
            reveal = revealCandidate
            position = candidatePosition
            firstIdentifying = firstIdentifyingCandidate
        }

        /// The handle of the import list `list`.
        public init(list: ImportList) {
            self.init(
                queries: { request in
                    BridgeListQueries(
                        importList: list,
                        request: request,
                        rows: { $0.importItems }
                    )
                },
                summary: { PulledValues(list.subscribeSummary()) },
                filterEntries: {
                    PulledValues(list.subscribeFilterEntries())
                },
                selection: { ImportSelectionQuery(list.subscribeSelection()) },
                viewChange: {
                    try await list.viewChange(
                        before: $0,
                        after: $1,
                        selected: $2
                    )
                },
                revealCandidate: {
                    try await list.revealCandidate(
                        request: $0,
                        candidateKey: $1
                    )
                },
                candidatePosition: {
                    try await list.candidatePosition(
                        request: $0,
                        candidateKey: $1
                    )
                },
                firstIdentifyingCandidate: {
                    try await list.firstIdentifyingCandidate(order: $0)
                }
            )
        }

        /// What showing the list `after` in place of the list `before`
        /// comes to for `selected`, as core decides it: where the list opens,
        /// and which of `selected` leave the selection.
        public func viewChange(
            before: BridgeImportListRequest,
            after: BridgeImportListRequest,
            selected: [Data]
        ) async throws -> BridgeImportListViewChange {
            try await change(before, after, selected)
        }

        /// What revealing the candidate from the list `request` shows: the
        /// request on the tab its placement puts it on, with the filters
        /// cleared and its group opened, and where it sits there. `nil` when
        /// no tab lists such a candidate.
        public func revealCandidate(
            _ request: BridgeImportListRequest,
            _ candidateKey: Data
        ) async throws -> BridgeImportListReveal? {
            try await reveal(request, candidateKey)
        }

        /// Where the list `request` shows the candidate. `nil` when it does
        /// not show it.
        public func candidatePosition(
            _ request: BridgeImportListRequest,
            _ candidateKey: Data
        ) async throws -> UInt64? {
            try await position(request, candidateKey)
        }

        /// The first candidate still being identified, in the order `order`.
        public func firstIdentifyingCandidate(
            _ order: BridgeImportListOrder
        ) async throws -> Data? {
            try await firstIdentifying(order)
        }
    }
#endif
