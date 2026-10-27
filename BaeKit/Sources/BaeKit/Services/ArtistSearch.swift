import Foundation
import Observation

/// An artist picker's live search of the library's artists: its text and
/// the windows of its list on screen change in place, and each value answers
/// every window at once, naming the revision of the view it was read for.
public struct ArtistSearchQuery: Sendable {
    public let setView:
        @Sendable (_ text: String, _ windows: [BridgeLibraryPageWindow]) throws
            -> UInt64
    public let next: @Sendable () async throws -> BridgeArtistSearchSnapshot
    public let cancel: @Sendable () async -> Void

    public init(
        setView:
            @escaping @Sendable (String, [BridgeLibraryPageWindow]) throws
            -> UInt64,
        next: @escaping @Sendable () async throws -> BridgeArtistSearchSnapshot,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setView = setView
        self.next = next
        self.cancel = cancel
    }

    init(_ subscription: any ArtistSearchSubscriptionProtocol) {
        self.init(
            setView: { try subscription.setView(query: $0, windows: $1) },
            next: { try await subscription.next() },
            cancel: { try? await subscription.cancel() }
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
                        let end = min(start + Int(window.limit), results.count)
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

extension BridgeArtistSearchResult: Identifiable {
    public var id: String { artist.artistId }
}

/// The pages of what an artist search finds, all served by one query for as
/// long as the picker is open.
///
/// A new text is a new view on the same query: `setText` drops the pages the
/// previous text's list registered, and the next list registers its own. The
/// whole set of registered windows under the current text is what the query
/// reads, and a value read for a view older than the text is not delivered.
public final class ArtistSearchPageSource: PageSource, @unchecked Sendable {
    private struct WindowKey: Hashable {
        let offset: UInt64
        let limit: UInt64
    }

    private struct Sink {
        let value:
            @MainActor @Sendable ([BridgeArtistSearchResult], Int) -> Void
        let error: @MainActor @Sendable (any Error) -> Void
    }

    private let query: ArtistSearchQuery
    private let lock = NSLock()
    private var text = ""
    /// The revision of the first view read for the current text: a value
    /// answering an older one was read for another text.
    private var textRevision: UInt64 = 0
    /// Moves with each text, so a page of a previous text's list that cancels
    /// late cannot take a window the current list registered.
    private var generation = 0
    private var sinks: [WindowKey: Sink] = [:]
    private var deliveries: Task<Void, Never>?
    /// The read failure this source died of, kept so a page registered after
    /// it hears about it instead of waiting on a loop that already returned.
    private var failure: (any Error)?

    public init(query: ArtistSearchQuery) {
        self.query = query
        deliveries = Task { [weak self] in
            await self?.deliver()
        }
    }

    deinit {
        close()
    }

    /// End the read: no value is delivered after this, and core drops the
    /// query.
    public func close() {
        deliveries?.cancel()
        let query = self.query
        Task { await query.cancel() }
    }

    /// Search for `text`, with no pages until its list registers them.
    public func setText(_ text: String) {
        let error: (any Error)? = lock.withLock {
            self.text = text
            generation += 1
            sinks = [:]
            do {
                textRevision = try query.setView(text, [])
                return nil
            }
            catch {
                return error
            }
        }
        if let error {
            failEveryPage(with: error)
        }
    }

    public func subscribe(
        offset: Int,
        limit: Int,
        onValue:
            @escaping @MainActor @Sendable (
                [BridgeArtistSearchResult], Int
            ) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> any PageSubscription {
        let key = WindowKey(offset: UInt64(offset), limit: UInt64(limit))
        let (failure, generation) = lock.withLock {
            sinks[key] = Sink(value: onValue, error: onError)
            return (self.failure, self.generation)
        }
        let page = PageWindow(source: self, key: key, generation: generation)
        if let failure {
            Task { @MainActor in onError(failure) }
            return page
        }
        push(failing: onError)
        return page
    }

    private func remove(_ key: WindowKey, generation: Int) {
        let removed = lock.withLock {
            guard generation == self.generation else { return false }
            sinks.removeValue(forKey: key)
            return true
        }
        if removed {
            push(failing: nil)
        }
    }

    /// The current text with every registered page as a window, in offset
    /// order, so the same pages make the same request.
    private func push(
        failing onError: (@MainActor @Sendable (any Error) -> Void)?
    ) {
        let error: (any Error)? = lock.withLock {
            let windows = sinks.keys
                .sorted { ($0.offset, $0.limit) < ($1.offset, $1.limit) }
                .map {
                    BridgeLibraryPageWindow(offset: $0.offset, limit: $0.limit)
                }
            do {
                _ = try query.setView(text, windows)
                return nil
            }
            catch {
                return error
            }
        }
        guard let error else { return }
        // The page that just registered waits on its first value, so it has
        // to hear about a request that never reached core.
        if let onError {
            Task { @MainActor in onError(error) }
        }
        else {
            failEveryPage(with: error)
        }
    }

    private func deliver() async {
        while !Task.isCancelled {
            do {
                let snapshot = try await query.next()
                let sinks: [WindowKey: Sink]? = lock.withLock {
                    guard snapshot.requestRevision >= textRevision else {
                        return nil
                    }
                    return self.sinks
                }
                guard let sinks else { continue }
                await MainActor.run {
                    for window in snapshot.windows {
                        let key = WindowKey(
                            offset: window.window.offset,
                            limit: window.window.limit
                        )
                        sinks[key]?.value(window.rows, Int(snapshot.totalCount))
                    }
                }
            }
            catch {
                if Task.isCancelled { return }
                failEveryPage(with: error)
                return
            }
        }
    }

    private func failEveryPage(with error: any Error) {
        let sinks = lock.withLock {
            failure = error
            return self.sinks
        }
        Task { @MainActor in
            for sink in sinks.values {
                sink.error(error)
            }
        }
    }

    private final class PageWindow: PageSubscription, @unchecked Sendable {
        private let source: ArtistSearchPageSource
        private let key: WindowKey
        private let generation: Int

        init(source: ArtistSearchPageSource, key: WindowKey, generation: Int) {
            self.source = source
            self.key = key
            self.generation = generation
        }

        func cancel() {
            source.remove(key, generation: generation)
        }
    }
}

/// The artists an artist search finds, a page at a time.
public typealias ArtistSearchList = PaginatedList<BridgeArtistSearchResult>

/// One artist picker's search while it is open: the list of what the text
/// typed last finds, read through the picker's one live query.
///
/// A new text waits out typing, then moves the query; the list it found
/// before stays until the new text's list has answered, so the results never
/// blank between keystrokes.
@MainActor
@Observable
public final class ArtistSearchResults {
    /// What the text typed last finds; nil until a text's list has answered.
    public private(set) var list: ArtistSearchList?
    /// A text's list has been asked for and has not answered yet.
    public private(set) var isSearching = false
    /// The read failed; the list shown, if any, is what it read before.
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private let source: ArtistSearchPageSource
    @ObservationIgnored
    private var pending: Task<Void, Never>?
    @ObservationIgnored
    private var generation = 0

    /// Open the picker's search through `open`, which opens the live query.
    public init(open: @Sendable () -> ArtistSearchQuery) {
        source = ArtistSearchPageSource(query: open())
    }

    /// Follow `text`: once typing pauses, the query moves to it and its list
    /// replaces the one shown.
    public func search(_ text: String) {
        pending?.cancel()
        generation += 1
        let current = generation
        pending = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(200))
            }
            catch {
                return
            }
            await self?.read(text, generation: current)
        }
    }

    private func read(_ text: String, generation current: Int) async {
        isSearching = true
        source.setText(text)
        let next = ArtistSearchList(
            pageSource: source,
            onError: { [weak self] error in
                self?.error = DisplayError(error)
            }
        )
        await next.loadInitial()
        guard generation == current else {
            next.cancel()
            return
        }
        list?.cancel()
        list = next
        error = next.initialLoadError
        isSearching = false
    }

    /// End the search: the picker is closing.
    public func close() {
        pending?.cancel()
        pending = nil
        generation += 1
        list?.cancel()
        source.close()
    }
}
