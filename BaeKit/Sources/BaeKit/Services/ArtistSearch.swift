import Foundation
import Observation

/// How an artist picker opens the list of the artists a text finds: each text
/// is a list of its own, whose windows, count and positions are live reads
/// like any other list's.
public typealias ArtistSearchOpener =
    @Sendable (_ text: String) -> BridgeListQueries<BridgeArtistSearchResult>

extension BridgeArtistSearchResult: Identifiable {
    public var id: String { artist.artistId }
}

/// The artists an artist search finds, a page at a time.
public typealias ArtistSearchList = PaginatedList<BridgeArtistSearchResult>

/// One artist picker's search while it is open: the list of what the text
/// typed last finds.
///
/// A new text waits out typing, then opens the list of its own; the list the
/// text before it found stays until the new one has answered, so the results
/// never blank between keystrokes.
@MainActor
@Observable
public final class ArtistSearchResults {
    /// What the text typed last finds; nil until a text's list has answered.
    public private(set) var list: ArtistSearchList?
    /// The read failed; the list shown, if any, is what it read before.
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
        list = next
        error = next.initialLoadError
    }

    /// End the search: the picker is closing.
    public func close() {
        pending?.cancel()
        pending = nil
        generation += 1
        list?.cancel()
    }
}
