import Foundation

// The import list is a desktop surface: bae-bridge compiles the import list's
// types only for desktop targets, so the iOS bindings do not have them.
#if os(macOS)

    /// One item of the import list, addressed by the key core computed for it.
    extension BridgeImportListItem: Identifiable {
        public var id: String {
            switch self {
            case .groupHeader(let stableKey, _, _, _, _),
                .candidate(let stableKey, _, _),
                .invalid(let stableKey, _):
                return stableKey
            }
        }
    }

    extension BridgeListRows {
        /// The items of an import list's window.
        fileprivate var importItems: [BridgeImportListItem] {
            guard case .import(let items, _) = self else {
                preconditionFailure(
                    "the import list delivered rows of another list"
                )
            }
            return items
        }

        /// The header of the group an import list's window starts inside.
        fileprivate var importLeadingHeader: BridgeImportListItem? {
            guard case .import(_, let header) = self else {
                preconditionFailure(
                    "the import list delivered rows of another list"
                )
            }
            return header
        }
    }

    /// The open import list: its windows, count and position as the generic
    /// list reads them, the reads of what surrounds the list, and the writes
    /// that change what it shows. Every read it makes ends with the list.
    public struct ImportListHandle: Sendable {
        /// The windows, count and position of the list's rows.
        public let queries: BridgeListQueries<BridgeImportListItem>
        /// The watched folders, whether Found has groups, and the narrowing.
        public let summary:
            @Sendable () -> PulledValues<BridgeImportQueueSummary>
        /// Found's filter entries with how many rows each holds.
        public let filterEntries:
            @Sendable () -> PulledValues<[BridgePendingFilterEntry]>
        /// What the rows the screen keeps selected hold and the actions the
        /// person can run on them, read for the keys the screen sets on it.
        public let selection: @Sendable () -> ImportSelectionQuery
        private let applyView:
            @Sendable (BridgeImportListView, [Data]) async throws
                -> BridgeImportListViewChange?
        private let fold:
            @Sendable (BridgeFolderReleaseDecisionKey, Bool, [Data])
                async throws -> [Data]
        private let foldAll: @Sendable (Bool, [Data]) async throws -> [Data]
        private let locate:
            @Sendable (Data) async throws -> BridgeImportCandidateListLocation?
        private let firstIdentifying: @Sendable () async throws -> Data?
        private let closeList: @Sendable () async throws -> Void

        public init(
            queries: BridgeListQueries<BridgeImportListItem>,
            summary:
                @escaping @Sendable () -> PulledValues<BridgeImportQueueSummary>,
            filterEntries:
                @escaping @Sendable () -> PulledValues<
                    [BridgePendingFilterEntry]
                >,
            selection: @escaping @Sendable () -> ImportSelectionQuery,
            setView:
                @escaping @Sendable (BridgeImportListView, [Data]) async throws
                -> BridgeImportListViewChange?,
            setGroupFolded:
                @escaping @Sendable (
                    BridgeFolderReleaseDecisionKey, Bool, [Data]
                ) async throws -> [Data],
            setAllGroupsFolded:
                @escaping @Sendable (Bool, [Data]) async throws -> [Data],
            locateCandidate:
                @escaping @Sendable (Data) async throws
                -> BridgeImportCandidateListLocation?,
            firstIdentifyingCandidate:
                @escaping @Sendable () async throws -> Data?,
            close: @escaping @Sendable () async throws -> Void = {}
        ) {
            self.queries = queries
            self.summary = summary
            self.filterEntries = filterEntries
            self.selection = selection
            applyView = setView
            fold = setGroupFolded
            foldAll = setAllGroupsFolded
            locate = locateCandidate
            firstIdentifying = firstIdentifyingCandidate
            closeList = close
        }

        /// The handle of the open list `list`.
        public init(list: ImportList) {
            self.init(
                queries: BridgeListQueries(
                    importList: list,
                    rows: { $0.importItems },
                    leadingHeader: { $0.importLeadingHeader }
                ),
                summary: { PulledValues(list.subscribeSummary()) },
                filterEntries: {
                    PulledValues(list.subscribeFilterEntries())
                },
                selection: { ImportSelectionQuery(list.subscribeSelection()) },
                setView: { try await list.setView(view: $0, selected: $1) },
                setGroupFolded: {
                    try await list.setGroupFolded(
                        group: $0,
                        folded: $1,
                        selected: $2
                    )
                },
                setAllGroupsFolded: {
                    try await list.setAllGroupsFolded(folded: $0, selected: $1)
                },
                locateCandidate: {
                    try await list.locateCandidate(candidateKey: $0)
                },
                firstIdentifyingCandidate: {
                    try await list.firstIdentifyingCandidate()
                },
                close: { try await list.cancel() }
            )
        }

        /// Show `view`, where the screen keeps the rows at `selected`
        /// selected, and answer where it opens, at the first of those rows
        /// it shows or the list's top, and which of them leave the
        /// selection: on another tab, filter or search, the ones the new view
        /// doesn't show. `nil` when it is the view the list already shows.
        /// The caller drops the windows it holds and opens new ones from that
        /// place.
        public func setView(
            _ view: BridgeImportListView,
            selected: [Data]
        ) async throws -> BridgeImportListViewChange? {
            try await applyView(view, selected)
        }

        /// Fold one group shut, or open it, and answer with the keys of
        /// `selected` whose rows the fold hid.
        public func setGroupFolded(
            _ group: BridgeFolderReleaseDecisionKey,
            _ folded: Bool,
            selected: [Data]
        ) async throws -> [Data] {
            try await fold(group, folded, selected)
        }

        /// Fold every group shut, or open them all, as one change, and answer
        /// with the keys of `selected` whose rows the fold hid.
        public func setAllGroupsFolded(
            _ folded: Bool,
            selected: [Data]
        ) async throws -> [Data] {
            try await foldAll(folded, selected)
        }

        /// Where the list shows the candidate after it clears the filters and
        /// opens the candidate's group: its tab, and the key a chain of
        /// windows starts at to show it. `nil` when the list
        /// lists no such candidate.
        public func locateCandidate(_ candidateKey: Data) async throws
            -> BridgeImportCandidateListLocation?
        {
            try await locate(candidateKey)
        }

        /// Close the list: every read opened from it ends, and its folds go.
        public func close() async throws {
            try await closeList()
        }

        /// The first candidate still being identified, in the list's order.
        public func firstIdentifyingCandidate() async throws -> Data? {
            try await firstIdentifying()
        }
    }
#endif
