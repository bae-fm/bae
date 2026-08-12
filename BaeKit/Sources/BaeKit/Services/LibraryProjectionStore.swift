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
    @ObservationIgnored
    private var searchTask: Task<Void, Never>?
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

    public func activateSearch(_ rawQuery: String) {
        let query = rawQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard searchQuery != query || searchTask == nil else { return }
        searchQuery = query
        searchTask?.cancel()
        if query.isEmpty {
            search = LibraryProjectionState()
            searchTask = nil
            return
        }
        search = LibraryProjectionState()
        searchTask = Task { [weak self, library] in
            do {
                try await Task.sleep(for: .milliseconds(300))
            }
            catch {
                return
            }
            for await result in library.searchResults(query) {
                guard !Task.isCancelled, self?.searchQuery == query else {
                    return
                }
                switch result {
                case .success(let value):
                    self?.search = LibraryProjectionState(
                        value: SearchResults(bridge: value, query: query),
                        delivered: true
                    )
                case .failure(let error):
                    self?.search = LibraryProjectionState(
                        value: self?.search.value,
                        delivered: self?.search.delivered ?? false,
                        error: DisplayError(error)
                    )
                }
            }
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

    public func deactivateSearch(_ rawQuery: String) {
        let query = rawQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard searchQuery == query else { return }
        searchTask?.cancel()
        searchTask = nil
        searchQuery = nil
    }
}
