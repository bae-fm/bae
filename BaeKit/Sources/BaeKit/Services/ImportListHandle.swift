import Foundation

// The import list is a desktop surface: its bridge types are built behind the
// bridge's desktop condition, which the iOS bindings leave out.
#if os(macOS)

    /// One item of the import list, addressed by the key core computed for it.
    extension BridgeImportListItem: Identifiable {
        public var id: String {
            switch self {
            case .groupHeader(let stableKey, _, _, _, _),
                .candidate(let stableKey, _, _),
                .invalid(let stableKey, _, _):
                return stableKey
            }
        }
    }

    extension BridgeListRows {
        /// The items of an import list's window; none for any other list's.
        fileprivate var importItems: [BridgeImportListItem] {
            if case .import(let items, _) = self {
                items
            }
            else {
                []
            }
        }

        /// The header of the group an import list's window starts inside.
        fileprivate var importLeadingHeader: BridgeImportListItem? {
            if case .import(_, let header) = self {
                header
            }
            else {
                nil
            }
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
        /// What the selection holds and can be told to do.
        public let selection:
            @Sendable () -> PulledValues<BridgeSelectionSummary>
        private let applyView:
            @Sendable (BridgeImportListView) async throws
                -> BridgeImportListOpening?
        private let fold:
            @Sendable (BridgeFolderReleaseDecisionKey, Bool) async throws ->
                Void
        private let foldAll: @Sendable (Bool) async throws -> Void
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
            selection:
                @escaping @Sendable () -> PulledValues<BridgeSelectionSummary>,
            setView:
                @escaping @Sendable (BridgeImportListView) async throws
                -> BridgeImportListOpening?,
            setGroupFolded:
                @escaping @Sendable (BridgeFolderReleaseDecisionKey, Bool)
                async throws -> Void,
            setAllGroupsFolded: @escaping @Sendable (Bool) async throws -> Void,
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
                selection: { PulledValues(list.subscribeSelection()) },
                setView: { try await list.setView(view: $0) },
                setGroupFolded: {
                    try await list.setGroupFolded(group: $0, folded: $1)
                },
                setAllGroupsFolded: {
                    try await list.setAllGroupsFolded(folded: $0)
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

        /// Show `view`, and answer where it opens: its first selected row,
        /// or the list's top. `nil` when it is the view the list already
        /// shows. The caller drops the windows it holds and opens new ones
        /// from that place.
        public func setView(_ view: BridgeImportListView) async throws
            -> BridgeImportListOpening?
        {
            try await applyView(view)
        }

        /// Fold one group shut, or open it.
        public func setGroupFolded(
            _ group: BridgeFolderReleaseDecisionKey,
            _ folded: Bool
        ) async throws {
            try await fold(group, folded)
        }

        /// Fold every group shut, or open them all, as one change.
        public func setAllGroupsFolded(_ folded: Bool) async throws {
            try await foldAll(folded)
        }

        /// Where the candidate sits once revealed at its current placement:
        /// its tab, with the filters cleared and its group opened, and the
        /// key a chain of windows starts at to show it. `nil` when the list
        /// lists no such candidate.
        public func locateCandidate(_ candidateKey: Data) async throws
            -> BridgeImportCandidateListLocation?
        {
            try await locate(candidateKey)
        }

        /// Close the list: every read opened from it ends, and its folds and
        /// selection go.
        public func close() async throws {
            try await closeList()
        }

        /// The first candidate still being identified, in the list's order.
        public func firstIdentifyingCandidate() async throws -> Data? {
            try await firstIdentifying()
        }
    }
#endif
