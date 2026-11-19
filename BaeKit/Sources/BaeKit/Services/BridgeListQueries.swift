import Foundation

/// One delivery of a list's view: the rows of the window core keeps around
/// the rows on screen, the first at position `start`.
public struct ListViewDelivery<Row: Sendable>: Sendable {
    /// The position of the first row.
    public let start: Int
    public let rows: [Row]

    public init(start: Int, rows: [Row]) {
        self.start = start
        self.rows = rows
    }

    /// The row at list position `position`, or nil if the window doesn't
    /// hold it.
    public func row(at position: Int) -> Row? {
        let at = position - start
        return rows.indices.contains(at) ? rows[at] : nil
    }
}

/// A live read of one list's view: core keeps one window of the list around
/// the rows on screen, which `setVisible` reports, and moves and sizes it.
public struct ListViewQuery<Row: Sendable>: Sendable {
    public let setVisible: @Sendable (_ first: UInt64, _ last: UInt64) -> Void
    public let values: PulledValues<ListViewDelivery<Row>>

    public init(
        setVisible: @escaping @Sendable (UInt64, UInt64) -> Void,
        values: PulledValues<ListViewDelivery<Row>>
    ) {
        self.setVisible = setVisible
        self.values = values
    }
}

/// The reads every list has: its view, opened with the rows on screen, and
/// its row count, each a live read.
public struct BridgeListQueries<Row: Sendable>: Sendable {
    public let view:
        @Sendable (_ first: UInt64, _ last: UInt64) throws -> ListViewQuery<Row>
    public let count: @Sendable () throws -> PulledValues<BridgeListCount>
    /// The album grid grouped by artist: each artist's heading with where its
    /// albums start. Other lists have none.
    public let sections:
        (@Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>)?

    public init(
        view:
            @escaping @Sendable (UInt64, UInt64) throws -> ListViewQuery<Row>,
        count: @escaping @Sendable () throws -> PulledValues<BridgeListCount>,
        sections: (
            @Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>
        )? = nil
    ) {
        self.view = view
        self.count = count
        self.sections = sections
    }
}

#if os(macOS)
    /// The two list calls, which `AppHandle` and the import list both make.
    protocol ListCalls: Sendable {
        func subscribeListView(first: UInt64, last: UInt64) throws
            -> ListViewSubscription
        func subscribeListCount() throws -> ListCountSubscription
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

        func subscribeListView(first: UInt64, last: UInt64) throws
            -> ListViewSubscription
        {
            try handle.subscribeListView(list: list, first: first, last: last)
        }

        func subscribeListCount() throws -> ListCountSubscription {
            try handle.subscribeListCount(list: list)
        }
    }

    /// The calls of the import list under `request`.
    private struct ImportListCalls: ListCalls {
        let list: ImportList
        let request: BridgeImportListRequest

        func subscribeListView(first: UInt64, last: UInt64) throws
            -> ListViewSubscription
        {
            try list.subscribeListView(
                request: request,
                first: first,
                last: last
            )
        }

        func subscribeListCount() throws -> ListCountSubscription {
            try list.subscribeListCount(request: request)
        }
    }

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
                rows: rows
            )
        }

        /// The reads of the import list `list` under `request`.
        public init(
            importList list: ImportList,
            request: BridgeImportListRequest,
            rows: @escaping @Sendable (BridgeListRows) -> [Row]
        ) {
            self.init(
                calls: ImportListCalls(list: list, request: request),
                sections: nil,
                rows: rows
            )
        }

        private init(
            calls: any ListCalls,
            sections: (
                @Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>
            )?,
            rows: @escaping @Sendable (BridgeListRows) -> [Row]
        ) {
            self.init(
                view: { first, last in
                    let subscription = try calls.subscribeListView(
                        first: first,
                        last: last
                    )
                    return ListViewQuery(
                        setVisible: {
                            subscription.setVisible(first: $0, last: $1)
                        },
                        values: PulledValues(subscription) { value in
                            ListViewDelivery(
                                start: Int(value.start),
                                rows: rows(value.rows)
                            )
                        }
                    )
                },
                count: {
                    PulledValues(try calls.subscribeListCount())
                },
                sections: sections
            )
        }
    }
#endif

extension ListViewDelivery: Equatable where Row: Equatable {}
