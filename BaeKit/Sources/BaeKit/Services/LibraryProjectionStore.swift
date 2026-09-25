import Foundation
import Observation

public struct LibraryProjectionState<Value: Sendable>: Sendable {
    public let value: Value?
    public let delivered: Bool
    public let error: DisplayError?

    public init(
        value: Value? = nil,
        delivered: Bool = false,
        error: DisplayError? = nil
    ) {
        self.value = value
        self.delivered = delivered
        self.error = error
    }
}

@MainActor
@Observable
public final class LibraryProjectionStore {
    public private(set) var composer =
        LibraryProjectionState<BridgeComposerDetail>()
    public private(set) var artist =
        LibraryProjectionState<BridgeArtistDetail>()
    public private(set) var work = LibraryProjectionState<BridgeWorkDetail>()
    public private(set) var search = LibraryProjectionState<SearchResults>()

    @ObservationIgnored
    private let library: Library
    /// Each detail pane's one read, moved to the item the pane shows.
    @ObservationIgnored
    private var composerReader: DetailReader<BridgeComposerDetail>?
    @ObservationIgnored
    private var artistReader: DetailReader<BridgeArtistDetail>?
    @ObservationIgnored
    private var workReader: DetailReader<BridgeWorkDetail>?
    /// The one live search the search field drives while it is open, and
    /// the loop taking its values.
    @ObservationIgnored
    private var liveSearch: LibrarySearch?
    @ObservationIgnored
    private var searchDeliveries: Task<Void, Never>?
    /// Waits out typing before the query moves, so a burst of keystrokes is
    /// one query change.
    @ObservationIgnored
    private var searchDebounce: Task<Void, Never>?
    @ObservationIgnored
    private var searchQuery: String?

    public init(library: Library) {
        self.library = library
    }

    public func activateComposer(_ id: String) {
        if composerReader?.id != id { composer = LibraryProjectionState() }
        composerReader =
            composerReader
            ?? detailReader(
                open: library.composerDetail,
                state: \.composer
            )
        composerReader?.show(id)
    }

    public func activateArtist(_ id: String) {
        if artistReader?.id != id { artist = LibraryProjectionState() }
        artistReader =
            artistReader
            ?? detailReader(
                open: library.artistDetail,
                state: \.artist
            )
        artistReader?.show(id)
    }

    public func activateWork(_ id: String) {
        if workReader?.id != id { work = LibraryProjectionState() }
        workReader =
            workReader
            ?? detailReader(
                open: library.workDetail,
                state: \.work
            )
        workReader?.show(id)
    }

    /// A detail pane's read, writing each value it delivers — and each
    /// failure, over the value already shown — into `state`.
    private func detailReader<Value: Sendable>(
        open: @escaping @Sendable () -> DetailQuery<Value>,
        state: ReferenceWritableKeyPath<
            LibraryProjectionStore, LibraryProjectionState<Value>
        >
    ) -> DetailReader<Value> {
        DetailReader(
            open: open,
            onValue: { [weak self] _, value in
                self?[keyPath: state] = LibraryProjectionState(
                    value: value,
                    delivered: true
                )
            },
            onError: { [weak self] _, error in
                guard let self else { return }
                let current = self[keyPath: state]
                self[keyPath: state] = LibraryProjectionState(
                    value: current.value,
                    delivered: current.delivered,
                    error: DisplayError(error)
                )
            }
        )
    }

    /// Follow the search field's text: the one live search moves to it once
    /// typing pauses. Empty text is no search, and reads nothing.
    public func activateSearch(_ rawQuery: String) {
        let query = rawQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard searchQuery != query else { return }
        searchQuery = query
        search = LibraryProjectionState()
        searchDebounce?.cancel()
        searchDebounce = nil
        if query.isEmpty {
            if let liveSearch { setSearchQuery("", on: liveSearch) }
            return
        }
        let live = openSearch()
        searchDebounce = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(300))
            }
            catch {
                return
            }
            guard self?.searchQuery == query else { return }
            self?.setSearchQuery(query, on: live)
        }
    }

    /// The live search, opened on first use. Its values are taken for as long
    /// as it stays open; each names the query it answers, and only an answer to
    /// the query standing now is shown.
    private func openSearch() -> LibrarySearch {
        if let liveSearch { return liveSearch }
        let live = library.librarySearch()
        liveSearch = live
        searchDeliveries = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let snapshot = try await live.next()
                    guard let self else {
                        await live.cancel()
                        return
                    }
                    guard !snapshot.query.isEmpty,
                        self.searchQuery == snapshot.query
                    else { continue }
                    self.search = LibraryProjectionState(
                        value: SearchResults(
                            bridge: snapshot.results,
                            query: snapshot.query
                        ),
                        delivered: true
                    )
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch {
                    guard let self else { return }
                    self.search = LibraryProjectionState(
                        value: self.search.value,
                        delivered: self.search.delivered,
                        error: DisplayError(error)
                    )
                }
            }
        }
        return live
    }

    private func setSearchQuery(_ query: String, on live: LibrarySearch) {
        do {
            try live.setQuery(query)
        }
        catch {
            search = LibraryProjectionState(error: DisplayError(error))
        }
    }

    public func deactivateComposer(_ id: String) {
        guard composerReader?.id == id else { return }
        composerReader?.clear()
    }

    public func deactivateArtist(_ id: String) {
        guard artistReader?.id == id else { return }
        artistReader?.clear()
    }

    public func deactivateWork(_ id: String) {
        guard workReader?.id == id else { return }
        workReader?.clear()
    }

    /// Close the live search: the search field is gone.
    public func deactivateSearch() {
        searchDebounce?.cancel()
        searchDebounce = nil
        searchDeliveries?.cancel()
        searchDeliveries = nil
        searchQuery = nil
        if let live = liveSearch {
            liveSearch = nil
            Task { await live.cancel() }
        }
    }
}
