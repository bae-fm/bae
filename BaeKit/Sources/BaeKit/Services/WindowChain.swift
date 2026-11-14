import Foundation

/// The windows a list reads, in the order of its rows: each a live read of a
/// range of the order, neighbours sharing a bound so every row is held once.
/// A window owns its subscription and only its last delivery, replaced
/// whole; the rows are those deliveries laid end to end from the position of
/// the chain's first row, which is its own live read of how many rows come
/// before the chain's start key.
///
/// The chain fits itself to what it delivers: an open window is closed at the
/// row past its limit, a closed window that rows were added to is split there,
/// a window that went empty is dropped into its neighbour. It grows one window
/// below or above and drops windows well beyond the rows on screen, which the
/// list tells it in `visible`.
@MainActor
final class WindowChain<Row: Sendable & Equatable> {
    /// The rows each window last delivered, laid end to end.
    private(set) var placed: [Placed<Row>] = []
    /// The positions on screen, which the chain stays around.
    var visible: ClosedRange<Int>?
    /// Whether any window has delivered since the chain started.
    private(set) var hasAnswered = false

    private let queries: BridgeListQueries<Row>
    private let pageSize: Int
    private let onChange: () -> Void
    private let onError: (any Error) -> Void

    private var windows: [ChainWindow<Row>] = []
    /// Where the chain's first row sits.
    private var start: StartPosition!

    init(
        queries: BridgeListQueries<Row>,
        pageSize: Int,
        onChange: @escaping () -> Void,
        onError: @escaping (any Error) -> Void
    ) {
        self.queries = queries
        self.pageSize = pageSize
        self.onChange = onChange
        self.onError = onError
        start = StartPosition(
            position: queries.position,
            onChange: { [weak self] in self?.startMoved() },
            onError: onError
        )
    }

    var isEmpty: Bool { windows.isEmpty }

    /// Where the laid-out rows end.
    private var end: Int {
        guard let last = placed.last else { return start.value }
        return last.start + last.rows.count
    }

    /// Whether the row at `position` is near enough the chain's rows for the
    /// chain to grow to it; a position farther is a jump. The last window of
    /// the list, once answered, reaches every position past it.
    func reaches(_ position: Int) -> Bool {
        let near = 2 * pageSize
        let low = start.key == nil ? Int.min : start.value - near
        var high = end + near
        if let last = windows.last, last.answered,
            case .forward(_, nil, _) = last.request
        {
            high = Int.max
        }
        return position >= low && position <= high
    }

    /// Replace the windows with one starting at `key` (the list's top when
    /// nil), standing `position` in for the count of the rows before it until
    /// that count answers.
    func restart(at key: [BridgeSortValue]?, position: Int) {
        for window in windows {
            window.stop()
        }
        hasAnswered = false
        start.follow(key, assuming: position)
        windows = [
            open(.forward(from: key, until: nil, limit: UInt64(pageSize)))
        ]
        layOut()
    }

    func stop() {
        for window in windows {
            window.stop()
        }
        windows = []
        start.stop()
        layOut()
    }

    // MARK: - Opening and closing windows

    private func open(_ request: BridgeListWindow) -> ChainWindow<Row> {
        let window = ChainWindow(request: request, queries: queries)
        let id = window.id
        window.deliveries = ScreenSubscription(
            window.query.values,
            onValue: { [weak self] delivery in
                self?.deliver(delivery, to: id)
            },
            onError: { [weak self] error in self?.onError(error) }
        )
        return window
    }

    private func remove(at index: Int) {
        windows[index].stop()
        windows.remove(at: index)
    }

    private func move(_ window: ChainWindow<Row>, to request: BridgeListWindow)
    {
        if let failure = window.move(to: request) {
            onError(failure)
        }
    }

    /// The count of the rows before the chain's first row answered.
    private func startMoved() {
        layOut()
        reconcile()
        onChange()
    }

    // MARK: - Deliveries

    private func deliver(_ delivery: ListWindowDelivery<Row>, to id: UUID) {
        // A value for a window the chain has since moved or dropped draws
        // nothing.
        guard let window = windows.first(where: { $0.id == id }),
            delivery.window == window.request
        else { return }
        window.rows = delivery.rows
        window.first = delivery.first
        window.beyond = delivery.beyond
        window.answered = true
        hasAnswered = true
        settle(window)
        layOut()
        reconcile()
        onChange()
    }

    /// Fit the chain to what `window` just delivered: close an open window,
    /// split one that holds more than it asked for, and drop one that went
    /// empty.
    private func settle(_ window: ChainWindow<Row>) {
        guard let index = windows.firstIndex(where: { $0 === window }) else {
            return
        }
        switch window.request {
        case .forward(let from, let until, let limit):
            if let beyond = window.beyond {
                // Rows past the limit: the window ends where its rows do. A
                // closed window also gives the rest of its range to a new
                // window below it.
                window.beyond = nil
                move(
                    window,
                    to: .forward(from: from, until: beyond, limit: limit)
                )
                if until != nil {
                    windows.insert(
                        open(
                            .forward(from: beyond, until: until, limit: limit)
                        ),
                        at: index + 1
                    )
                }
            }
            else if window.rows.isEmpty, until != nil, windows.count > 1 {
                dropEmpty(window, at: index)
            }
        case .backward(let until, let limit):
            if window.rows.isEmpty {
                if windows.count > 1 { remove(at: index) }
                return
            }
            // The rows above the chain's first row: the chain now starts at
            // them, at the list's top when nothing lies above them.
            let from = window.beyond == nil ? nil : window.first
            window.beyond = nil
            move(window, to: .forward(from: from, until: until, limit: limit))
            start.follow(
                from,
                assuming: max(0, start.value - window.rows.count)
            )
        }
    }

    /// Drop a window that holds no rows: the window above takes its range's
    /// end, or the window below takes its start when it is the first.
    private func dropEmpty(_ window: ChainWindow<Row>, at index: Int) {
        guard case .forward(let from, let until, _) = window.request else {
            return
        }
        if index > 0 {
            let above = windows[index - 1]
            switch above.request {
            case .forward(let aboveFrom, _, let limit):
                move(
                    above,
                    to: .forward(from: aboveFrom, until: until, limit: limit)
                )
            case .backward(_, let limit):
                if let until {
                    move(above, to: .backward(until: until, limit: limit))
                }
            }
        }
        else if index + 1 < windows.count {
            let below = windows[index + 1]
            if case .forward(_, let belowUntil, let limit) = below.request {
                move(
                    below,
                    to: .forward(from: from, until: belowUntil, limit: limit)
                )
                start.follow(from, assuming: start.value)
            }
        }
        remove(at: index)
    }

    // MARK: - Following the rows on screen

    /// Grow, trim and keep the chain around the rows on screen. A window is
    /// added below or above only when the one next to it has answered, so its
    /// bound and its row count are known.
    func reconcile() {
        guard let visible, !windows.isEmpty else { return }
        let page = pageSize
        if let last = windows.last, last.answered,
            case .forward(_, let until?, let limit) = last.request,
            end <= visible.upperBound + page
        {
            windows.append(
                open(.forward(from: until, until: nil, limit: limit))
            )
        }
        if let top = windows.first, top.answered,
            case .forward(let from?, _, _) = top.request,
            start.value > 0, start.value > visible.lowerBound - page
        {
            windows.insert(
                open(.backward(until: from, limit: UInt64(page))),
                at: 0
            )
        }
        trim(around: visible)
        layOut()
    }

    /// Drop windows wholly beyond two pages from the rows on screen.
    private func trim(around visible: ClosedRange<Int>) {
        layOut()
        let keepFrom = visible.lowerBound - 2 * pageSize
        let keepTo = visible.upperBound + 2 * pageSize
        var position = start.value
        var trimmedTop = false
        while windows.count > 1, let top = windows.first, top.answered,
            case .forward(_, _?, _) = top.request,
            position + top.rows.count <= keepFrom
        {
            position += top.rows.count
            remove(at: 0)
            trimmedTop = true
        }
        if trimmedTop, let top = windows.first,
            case .forward(let from, _, _) = top.request
        {
            start.follow(from, assuming: position)
        }
        layOut()
        while windows.count > 1, placed.count == windows.count,
            let start = placed.last?.start, start > keepTo
        {
            remove(at: windows.count - 1)
            layOut()
        }
    }

    // MARK: - Where the rows sit

    /// Lay the windows' last deliveries end to end from the chain's start
    /// position. A window that has not answered yet ends the laying, except a
    /// first window reading upward, whose rows are not what the start position
    /// stands at.
    private func layOut() {
        var next: [Placed<Row>] = []
        var position = start.value
        for (index, window) in windows.enumerated() {
            if !window.answered {
                if index == 0, case .backward = window.request { continue }
                break
            }
            next.append(Placed(start: position, rows: window.rows))
            position += window.rows.count
        }
        placed = next
    }
}

/// The rows of one window, with the position of the first.
struct Placed<Row: Equatable>: Equatable {
    let start: Int
    let rows: [Row]
}

/// One window of the chain: its request, its subscription, and what its last
/// delivery held.
@MainActor
final class ChainWindow<Row: Sendable> {
    let id = UUID()
    private(set) var request: BridgeListWindow
    let query: ListWindowQuery<Row>
    var deliveries: ScreenSubscription<PulledValues<ListWindowDelivery<Row>>>?
    var rows: [Row] = []
    var first: [BridgeSortValue]?
    var beyond: [BridgeSortValue]?
    /// Whether any delivery has landed. A window moved to a new request keeps
    /// its last delivery until the new one replaces it whole.
    var answered = false

    init(request: BridgeListWindow, queries: BridgeListQueries<Row>) {
        self.request = request
        query = queries.window(request)
    }

    /// Read `request` from now on; the failure to send it, if any.
    func move(to request: BridgeListWindow) -> (any Error)? {
        self.request = request
        do {
            try query.setWindow(request)
            return nil
        }
        catch {
            return error
        }
    }

    func stop() {
        deliveries?.stop()
        deliveries = nil
    }
}
