import Foundation
import Observation

/// One composer's page rows: the list a composer's page draws, opened for
/// that composer alone.
public struct ComposerPageRows {
    public let artistId: String
    public let list: ComposerDetailRowList
}

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
    /// The rows of the composer page shown now, once their first page has
    /// answered; another composer opens a list of its own.
    public private(set) var composerRows: ComposerPageRows?
    public private(set) var artist =
        LibraryProjectionState<BridgeArtistDetail>()
    public private(set) var work = LibraryProjectionState<BridgeWorkDetail>()
    public private(set) var search = LibraryProjectionState<SearchResults>()
    /// Every playlist this device holds, read from the first
    /// `activatePlaylists()` until the library closes: the Playlists view and
    /// a track's Add to playlist menu both read it.
    public internal(set) var playlists =
        LibraryProjectionState<BridgePlaylistListing>()
    public internal(set) var playlist =
        LibraryProjectionState<BridgePlaylistDetail>()

    @ObservationIgnored
    let library: Library
    /// Each detail pane's one read, moved to the item the pane shows while it
    /// shows one and closed when it stops.
    @ObservationIgnored
    private var composerReader: DetailReader<BridgeComposerDetail>?
    /// The composer whose rows are opening or shown, and the first read of
    /// a list opening for it.
    @ObservationIgnored
    private var composerRowsArtistId: String?
    @ObservationIgnored
    private var composerRowsLoad: Task<Void, Never>?
    @ObservationIgnored
    private var artistReader: DetailReader<BridgeArtistDetail>?
    @ObservationIgnored
    private var workReader: DetailReader<BridgeWorkDetail>?
    /// The playlist page's read and the pulls of the playlists, which
    /// `LibraryProjectionStore+Playlists.swift` drives.
    @ObservationIgnored
    var playlistReader: DetailReader<BridgePlaylistDetail>?
    @ObservationIgnored
    var playlistDeliveries:
        ScreenSubscription<PulledValues<BridgePlaylistListing>>?
    /// The one live search the search field drives while it is open, and
    /// the pulls taking its values.
    @ObservationIgnored
    private var liveSearch: LibrarySearch?
    @ObservationIgnored
    private var searchDeliveries:
        ScreenSubscription<PulledValues<BridgeLibrarySearchSnapshot>>?
    /// Waits out typing before the query moves, so a burst of keystrokes is
    /// one query change.
    @ObservationIgnored
    private var searchDebounce: Task<Void, Never>?
    @ObservationIgnored
    private var searchQuery: String?

    /// The clock the wait out of typing is measured on.
    @ObservationIgnored
    private let clock: any Clock<Duration>

    public init(
        library: Library,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.library = library
        self.clock = clock
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
        openComposerRows(id)
    }

    /// Open `id`'s page rows unless they are open already; the list shown
    /// stays until the new one's first page has answered.
    private func openComposerRows(_ id: String) {
        guard composerRowsArtistId != id else { return }
        composerRowsArtistId = id
        composerRowsLoad?.cancel()
        let list = ComposerDetailRowList(
            queries: library.composerPageList(id),
            onError: { [weak self] error in
                guard let self else { return }
                composer = LibraryProjectionState(
                    value: composer.value,
                    delivered: composer.delivered,
                    error: DisplayError(error)
                )
            }
        )
        composerRowsLoad = Task { [weak self] in
            await list.loadInitial()
            guard let self, !Task.isCancelled, composerRowsArtistId == id
            else {
                list.cancel()
                return
            }
            composerRows?.list.cancel()
            composerRows = ComposerPageRows(artistId: id, list: list)
        }
    }

    private func closeComposerRows() {
        composerRowsLoad?.cancel()
        composerRowsLoad = nil
        composerRowsArtistId = nil
        composerRows?.list.cancel()
        composerRows = nil
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
    func detailReader<Value: Sendable>(
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
            if let liveSearch { setSearchQuery(nil, on: liveSearch) }
            return
        }
        let live = openSearch()
        let clock = clock
        searchDebounce = Task { [weak self] in
            do {
                try await clock.sleep(for: .milliseconds(300))
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
        searchDeliveries = ScreenSubscription(
            live.values,
            onValue: { [weak self] snapshot in self?.applySearch(snapshot) },
            onError: { [weak self] error in self?.searchFailed(error) }
        )
        return live
    }

    private func applySearch(_ snapshot: BridgeLibrarySearchSnapshot) {
        guard let answered = snapshot.query, searchQuery == answered else {
            return
        }
        search = LibraryProjectionState(
            value: SearchResults(
                bridge: snapshot.results,
                query: answered
            ),
            delivered: true
        )
    }

    private func searchFailed(_ error: any Error) {
        search = LibraryProjectionState(
            value: search.value,
            delivered: search.delivered,
            error: DisplayError(error)
        )
    }

    private func setSearchQuery(_ query: String?, on live: LibrarySearch) {
        do {
            try live.setQuery(query)
        }
        catch {
            search = LibraryProjectionState(error: DisplayError(error))
        }
    }

    public func deactivateComposer(_ id: String) {
        guard composerReader?.id == id else { return }
        composerReader?.close()
        closeComposerRows()
    }

    public func deactivateArtist(_ id: String) {
        guard artistReader?.id == id else { return }
        artistReader?.close()
    }

    public func deactivateWork(_ id: String) {
        guard workReader?.id == id else { return }
        workReader?.close()
    }

    /// Close the live search: the search field is gone.
    public func deactivateSearch() {
        searchDebounce?.cancel()
        searchDebounce = nil
        searchDeliveries?.stop()
        searchDeliveries = nil
        searchQuery = nil
        liveSearch = nil
    }
}
