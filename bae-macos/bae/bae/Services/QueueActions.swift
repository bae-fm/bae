import BaeKit
import Foundation

/// Resolves album/track ids to flat track ids and applies a queue mutation,
/// surfacing resolve failures through the shared error path. Shared by the
/// queue's drops (`MainAppView`) and the album grid's bulk menu
/// (`LibraryView`) so both take the same resolve-then-apply route: album ids
/// expand to the primary release's tracks, track ids pass through, and the
/// grid's albums go in the grid's order, which core reads.
@MainActor
final class QueueActions {
    private let library: Library
    private let queue: Queue
    private let uiStore: UiStore

    init(library: Library, queue: Queue, uiStore: UiStore) {
        self.library = library
        self.queue = queue
        self.uiStore = uiStore
    }

    /// Append the grid's albums `albumIds` to the end of the manual lane, in
    /// the grid's order under `order`.
    func addAlbumsToQueue(_ albumIds: [String], order: AlbumGridOrder) {
        resolve(albums: albumIds, order: order) { [queue] trackIds in
            queue.addToQueue(trackIds)
        }
    }

    /// Insert the grid's albums `albumIds` at the front of the manual lane, in
    /// the grid's order under `order`.
    func addAlbumsNext(_ albumIds: [String], order: AlbumGridOrder) {
        resolve(albums: albumIds, order: order) { [queue] trackIds in
            queue.addNext(trackIds)
        }
    }

    /// Insert what was dropped at `index` in the manual lane: each string an
    /// album drag's payload, whose albums go in the grid's order under
    /// `order`, or a track's id.
    func insertDropped(
        _ dropped: [String],
        at index: Int,
        order: AlbumGridOrder
    ) {
        resolveDropped(dropped, order: order) { [queue] trackIds in
            queue.insertInQueue(trackIds, UInt32(index))
        }
    }

    /// Append what was dropped to the end of the manual lane, as
    /// `insertDropped` reads it.
    func addDropped(_ dropped: [String], order: AlbumGridOrder) {
        resolveDropped(dropped, order: order) { [queue] trackIds in
            queue.addToQueue(trackIds)
        }
    }

    private func resolve(
        albums albumIds: [String],
        order: AlbumGridOrder,
        then apply: @escaping ([String]) -> Void
    ) {
        run(apply) { library in
            let albums = try await order.albums(albumIds, in: library)
            return try await library.resolveToTrackIds(albums.map(\.id))
        }
    }

    private func resolveDropped(
        _ dropped: [String],
        order: AlbumGridOrder,
        then apply: @escaping ([String]) -> Void
    ) {
        run(apply) { library in
            var ids: [String] = []
            for payload in dropped {
                if let albumIds = AlbumDragPayload.albums(in: payload) {
                    ids += try await order.albums(albumIds, in: library)
                        .map(\.id)
                }
                else {
                    ids.append(payload)
                }
            }
            return try await library.resolveToTrackIds(ids)
        }
    }

    /// Resolve tracks through `read` and apply them, surfacing a failure.
    private func run(
        _ apply: @escaping ([String]) -> Void,
        _ read: @escaping (Library) async throws -> [String]
    ) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                let trackIds = try await read(library)
                guard !trackIds.isEmpty else {
                    return
                }
                apply(trackIds)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

/// The album grid's order: its sort, and whether it lists albums under each
/// of their artists. Core orders a selection of the grid's albums by it.
struct AlbumGridOrder: Sendable {
    let sort: [BridgeSortCriterion]
    let byArtist: Bool

    @MainActor
    init(session: LibraryBrowseSession) {
        sort = session.albums.sortCriteria
        byArtist = session.albumGrouping.byArtist
    }

    /// The albums of `albumIds` the grid lists, in its order, read from core.
    func albums(_ albumIds: [String], in library: Library) async throws
        -> [BridgeAlbum]
    {
        try await library.albumsInBrowseOrder(sort, albumIds, byArtist)
    }
}
