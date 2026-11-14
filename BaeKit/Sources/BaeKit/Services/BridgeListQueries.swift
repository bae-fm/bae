import Foundation

/// One delivery of a list window: the rows of the range it answers, with the
/// keys that chain it to its neighbours.
public struct ListWindowDelivery<Row: Sendable>: Sendable {
    /// The request this answers. A screen that has since moved the window
    /// drops the value.
    public let window: BridgeListWindow
    public let rows: [Row]
    /// The sort values of the first row; nil for a window of no rows.
    public let first: [BridgeSortValue]?
    /// The sort values of the row past the window's far end; nil when the
    /// window reached its bound, the list's end or the list's top.
    public let beyond: [BridgeSortValue]?
    /// A row that belongs above the window's first row without being in the
    /// window: the import list's header of the group the window starts
    /// inside. Nil for a list with no such row.
    public let leadingHeader: Row?

    public init(
        window: BridgeListWindow,
        rows: [Row],
        first: [BridgeSortValue]?,
        beyond: [BridgeSortValue]?,
        leadingHeader: Row? = nil
    ) {
        self.window = window
        self.rows = rows
        self.first = first
        self.beyond = beyond
        self.leadingHeader = leadingHeader
    }
}

/// A live read of one window of a list, moved in place with `setWindow`.
public struct ListWindowQuery<Row: Sendable>: Sendable {
    public let setWindow: @Sendable (BridgeListWindow) throws -> Void
    public let values: PulledValues<ListWindowDelivery<Row>>

    public init(
        setWindow: @escaping @Sendable (BridgeListWindow) throws -> Void,
        values: PulledValues<ListWindowDelivery<Row>>
    ) {
        self.setWindow = setWindow
        self.values = values
    }
}

/// A live read of how many rows come before the row at a key, moved in place
/// with `setKey`.
public struct ListPositionQuery: Sendable {
    public let setKey: @Sendable ([BridgeSortValue]) throws -> Void
    public let values: PulledValues<BridgeListPosition>

    public init(
        setKey: @escaping @Sendable ([BridgeSortValue]) throws -> Void,
        values: PulledValues<BridgeListPosition>
    ) {
        self.setKey = setKey
        self.values = values
    }
}

/// The reads of one list, which every list shares: each window of it is a
/// live read of its own, its count and the position of a key in it are
/// others, and a jump to a position is a plain read of the key there.
public struct BridgeListQueries<Row: Sendable>: Sendable {
    public let window:
        @Sendable (_ window: BridgeListWindow) -> ListWindowQuery<Row>
    public let count: @Sendable () -> PulledValues<BridgeListCount>
    public let position:
        @Sendable (_ key: [BridgeSortValue]) -> ListPositionQuery
    public let keyAt:
        @Sendable (_ position: UInt64) async throws -> [BridgeSortValue]?
    /// The album grid grouped by artist: each artist's heading with where its
    /// albums start. Other lists have none.
    public let sections:
        (@Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>)?

    public init(
        window:
            @escaping @Sendable (BridgeListWindow) -> ListWindowQuery<Row>,
        count: @escaping @Sendable () -> PulledValues<BridgeListCount>,
        position: @escaping @Sendable ([BridgeSortValue]) -> ListPositionQuery,
        keyAt:
            @escaping @Sendable (UInt64) async throws -> [BridgeSortValue]?,
        sections: (
            @Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>
        )? = nil
    ) {
        self.window = window
        self.count = count
        self.position = position
        self.keyAt = keyAt
        self.sections = sections
    }
}

#if os(macOS)
    /// The four list calls, which `AppHandle` and the import list's handle
    /// both make.
    protocol ListCalls: Sendable {
        func subscribeListWindow(window: BridgeListWindow)
            -> ListWindowSubscription
        func subscribeListCount() -> ListCountSubscription
        func subscribeListPosition(key: [BridgeSortValue])
            -> ListPositionSubscription
        func listKeyAt(position: UInt64) async throws -> [BridgeSortValue]?
    }

    extension AppHandleProtocol {
        /// The calls of `list`, as `ListCalls` makes them.
        func calls(of list: BridgeList) -> any ListCalls {
            AppHandleListCalls(handle: self, list: list)
        }
    }

    private struct AppHandleListCalls: ListCalls {
        let handle: any AppHandleProtocol
        let list: BridgeList

        func subscribeListWindow(window: BridgeListWindow)
            -> ListWindowSubscription
        {
            handle.subscribeListWindow(list: list, window: window)
        }

        func subscribeListCount() -> ListCountSubscription {
            handle.subscribeListCount(list: list)
        }

        func subscribeListPosition(key: [BridgeSortValue])
            -> ListPositionSubscription
        {
            handle.subscribeListPosition(list: list, key: key)
        }

        func listKeyAt(position: UInt64) async throws -> [BridgeSortValue]? {
            try await handle.listKeyAt(list: list, position: position)
        }
    }

    extension ImportList: ListCalls {}

    extension BridgeListQueries {
        /// The reads of `list` through `handle`, taking its rows out of the
        /// variant of `BridgeListRows` they come in.
        public init(
            handle: any AppHandleProtocol,
            list: BridgeList,
            sections: (
                @Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>
            )? =
                nil,
            rows: @escaping @Sendable (BridgeListRows) -> [Row]
        ) {
            self.init(
                calls: handle.calls(of: list),
                sections: sections,
                rows: rows,
                leadingHeader: nil
            )
        }

        /// The reads of the import list `list`, which names its own list.
        public init(
            importList list: ImportList,
            rows: @escaping @Sendable (BridgeListRows) -> [Row],
            leadingHeader: @escaping @Sendable (BridgeListRows) -> Row?
        ) {
            self.init(
                calls: list,
                sections: nil,
                rows: rows,
                leadingHeader: leadingHeader
            )
        }

        private init(
            calls: any ListCalls,
            sections: (
                @Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>
            )?,
            rows: @escaping @Sendable (BridgeListRows) -> [Row],
            leadingHeader: (@Sendable (BridgeListRows) -> Row?)?
        ) {
            self.init(
                window: { window in
                    let subscription = calls.subscribeListWindow(
                        window: window
                    )
                    return ListWindowQuery(
                        setWindow: { try subscription.setWindow(window: $0) },
                        values: PulledValues(subscription) { value in
                            ListWindowDelivery(
                                window: value.window,
                                rows: rows(value.rows),
                                first: value.first,
                                beyond: value.beyond,
                                leadingHeader: leadingHeader?(value.rows)
                            )
                        }
                    )
                },
                count: {
                    PulledValues(calls.subscribeListCount())
                },
                position: { key in
                    let subscription = calls.subscribeListPosition(key: key)
                    return ListPositionQuery(
                        setKey: { try subscription.setKey(key: $0) },
                        values: PulledValues(subscription)
                    )
                },
                keyAt: {
                    try await calls.listKeyAt(position: $0)
                },
                sections: sections
            )
        }
    }
#endif
