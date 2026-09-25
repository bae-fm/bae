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
    @ObservationIgnored
    private var composerTask: Task<Void, Never>?
    @ObservationIgnored
    private var artistTask: Task<Void, Never>?
    @ObservationIgnored
    private var workTask: Task<Void, Never>?
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
    private var composerId: String?
    @ObservationIgnored
    private var artistId: String?
    @ObservationIgnored
    private var workId: String?
    @ObservationIgnored
    private var searchQuery: String?

    public init(library: Library) {
        self.library = library
    }

    public func activateComposer(_ id: String) {
        guard composerId != id || composerTask == nil else { return }
        composerId = id
        composer = LibraryProjectionState()
        composerTask?.cancel()
        composerTask = Task { [weak self, library] in
            for await result in library.composerDetails(id) {
                guard !Task.isCancelled, self?.composerId == id else { return }
                switch result {
                case .success(let value):
                    self?.composer = LibraryProjectionState(
                        value: value,
                        delivered: true
                    )
                case .failure(let error):
                    self?.composer = LibraryProjectionState(
                        value: self?.composer.value,
                        delivered: self?.composer.delivered ?? false,
                        error: DisplayError(error)
                    )
                }
            }
        }
    }

    public func activateArtist(_ id: String) {
        guard artistId != id || artistTask == nil else { return }
        artistId = id
        artist = LibraryProjectionState()
        artistTask?.cancel()
        artistTask = Task { [weak self, library] in
            for await result in library.artistDetails(id) {
                guard !Task.isCancelled, self?.artistId == id else { return }
                switch result {
                case .success(let value):
                    self?.artist = LibraryProjectionState(
                        value: value,
                        delivered: true
                    )
                case .failure(let error):
                    self?.artist = LibraryProjectionState(
                        value: self?.artist.value,
                        delivered: self?.artist.delivered ?? false,
                        error: DisplayError(error)
                    )
                }
            }
        }
    }

    public func activateWork(_ id: String) {
        guard workId != id || workTask == nil else { return }
        workId = id
        work = LibraryProjectionState()
        workTask?.cancel()
        workTask = Task { [weak self, library] in
            for await result in library.workDetails(id) {
                guard !Task.isCancelled, self?.workId == id else { return }
                switch result {
                case .success(let value):
                    self?.work = LibraryProjectionState(
                        value: value,
                        delivered: true
                    )
                case .failure(let error):
                    self?.work = LibraryProjectionState(
                        value: self?.work.value,
                        delivered: self?.work.delivered ?? false,
                        error: DisplayError(error)
                    )
                }
            }
        }
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
        guard composerId == id else { return }
        composerTask?.cancel()
        composerTask = nil
        composerId = nil
    }

    public func deactivateArtist(_ id: String) {
        guard artistId == id else { return }
        artistTask?.cancel()
        artistTask = nil
        artistId = nil
    }

    public func deactivateWork(_ id: String) {
        guard workId == id else { return }
        workTask?.cancel()
        workTask = nil
        workId = nil
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
