import Foundation
import Observation

/// How an artist picker opens the list of the artists that match a text: the
/// picker opens a separate list for each text, whose view and count are live
/// reads like any other list's.
public typealias ArtistSearchOpener =
    @Sendable (_ text: String) -> BridgeListQueries<BridgeArtistSearchResult>

extension BridgeArtistSearchResult: Identifiable {
    public var id: String { artist.artistId }
}

/// The artists that match an artist search's text, a page at a time.
public typealias ArtistSearchList = PaginatedList<BridgeArtistSearchResult>

/// One artist picker's search while it is open: the list of the artists that
/// match the text typed last.
///
/// Once typing pauses, ArtistSearchResults opens the new text's list, and
/// keeps showing the previous text's list until the new list's query returns
/// its first rows, so the results never blank between keystrokes. When the
/// new text's first read fails, the picker shows that failure in place of the
/// list.
@MainActor
@Observable
public final class ArtistSearchResults {
    /// What the text typed last finds; nil until the first read of a text's
    /// list returns, and nil when that first read failed.
    public private(set) var list: ArtistSearchList?
    /// The error from the last text's read: either its first read failed and
    /// no list is shown, or a later read of the shown list failed.
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private let opener: ArtistSearchOpener
    @ObservationIgnored
    private var pending: Task<Void, Never>?
    @ObservationIgnored
    private var generation = 0
    /// The clock the wait out of typing is measured on.
    @ObservationIgnored
    private let clock: any Clock<Duration>

    /// Open the picker's search through `open`, which opens the list of the
    /// artists a text finds.
    public init(
        open: @escaping ArtistSearchOpener,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.opener = open
        self.clock = clock
    }

    /// Follow `text`: once typing pauses, its list replaces the one shown.
    public func search(_ text: String) {
        pending?.cancel()
        generation += 1
        let current = generation
        let clock = clock
        pending = Task { [weak self] in
            do {
                try await clock.sleep(for: .milliseconds(200))
            }
            catch {
                return
            }
            await self?.read(text, generation: current)
        }
    }

    private func read(_ text: String, generation current: Int) async {
        let next = ArtistSearchList(
            queries: opener(text),
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
        if let failure = next.initialLoadError {
            next.cancel()
            list = nil
            error = failure
            return
        }
        list = next
        error = nil
    }

    /// End the search: the picker is closing.
    public func close() {
        pending?.cancel()
        pending = nil
        generation += 1
        list?.cancel()
    }
}
