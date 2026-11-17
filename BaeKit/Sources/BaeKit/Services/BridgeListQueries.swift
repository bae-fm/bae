import Foundation

/// One delivery of a list's view: the rows of the window core keeps around
/// the rows on screen, the first at position `start`.
public struct ListViewDelivery<Row: Sendable>: Sendable {
    /// The position of the first row.
    public let start: Int
    public let rows: [Row]
    /// A row that belongs above the window's first row without being in the
    /// window: the import list's header of the group the window starts
    /// inside. Nil for a list with no such row.
    public let leadingHeader: Row?

    public init(start: Int, rows: [Row], leadingHeader: Row? = nil) {
        self.start = start
        self.rows = rows
        self.leadingHeader = leadingHeader
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
        @Sendable (_ first: UInt64, _ last: UInt64) -> ListViewQuery<Row>
    public let count: @Sendable () -> PulledValues<BridgeListCount>
    /// The album grid grouped by artist: each artist's heading with where its
    /// albums start. Other lists have none.
    public let sections:
        (@Sendable () -> PulledValues<[BridgeLibraryBrowseSection]>)?

    public init(
        view:
            @escaping @Sendable (UInt64, UInt64) -> ListViewQuery<Row>,
        count: @escaping @Sendable () -> PulledValues<BridgeListCount>,
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
    /// The two list calls, which `AppHandle` and the import list's handle
    /// both make.
    protocol ListCalls: Sendable {
        func subscribeListView(first: UInt64, last: UInt64)
            -> ListViewSubscription
        func subscribeListCount() -> ListCountSubscription
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

        func subscribeListView(first: UInt64, last: UInt64)
            -> ListViewSubscription
        {
            handle.subscribeListView(list: list, first: first, last: last)
        }

        func subscribeListCount() -> ListCountSubscription {
            handle.subscribeListCount(list: list)
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
                view: { first, last in
                    let subscription = calls.subscribeListView(
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
                                rows: rows(value.rows),
                                leadingHeader: leadingHeader?(value.rows)
                            )
                        }
                    )
                },
                count: {
                    PulledValues(calls.subscribeListCount())
                },
                sections: sections
            )
        }
    }
#endif

extension ListViewDelivery: Equatable where Row: Equatable {}
