import Combine
import Foundation
import Observation
import os.log

private let logger = Logger.bae("LibraryStore")

/// Describes a library-shape change broadcast on
/// `LibraryStore.libraryShapeSubject`.
///
/// Each variant carries the ids needed for a subscriber to decide
/// whether its list is affected. Library-scoped grids only care about
/// album-level variants; storage grids care about both album- and
/// release-level variants because releases are their rows.
///
/// Updates are included because metadata changes (rename, year edit)
/// can move a row to a different sort position. The reducer can't tell
/// whether a given update touched a sort-affecting field, so it fires
/// on every update; subscribers always invalidate, and `invalidate()`
/// is cheap (the old list stays visible during the refetch).
enum LibraryShapeChange {
    case albumAdded(albumId: String)
    case albumUpdated(albumId: String)
    case albumRemoved(albumId: String)
    case releaseAdded(albumId: String, releaseId: String)
    case releaseUpdated(albumId: String, releaseId: String)
    case releaseRemoved(albumId: String, releaseId: String)
}

// MARK: - Album page sources

/// Page source backed by the `Library` domain service for the full
/// library grid.
struct LibraryAlbumPageSource: PageSource {
    let library: Library
    let sort: [BridgeSortCriterion]

    func count() async throws -> Int {
        try Int(library.getAlbumCount())
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        try library.getAlbumPage(sort, UInt64(offset), UInt64(limit))
    }
}

/// In-memory page source for album previews and tests. Expects rows to
/// arrive pre-sorted; serves contiguous slices.
struct AlbumPreviewPageSource: PageSource {
    let albums: [BridgeAlbum]

    func count() async throws -> Int {
        albums.count
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
    }
}

struct LibraryComposerPageSource: PageSource {
    let library: Library
    let sort: BridgeComposerSortCriterion

    func count() async throws -> Int {
        try Int(library.getComposerCount())
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeComposerSummary] {
        try library.getComposerPage(sort, UInt64(offset), UInt64(limit))
    }
}

extension BridgeComposerSummary: Identifiable {
    public var id: String { artistId }
}

// MARK: - Storage page source

/// Page source backed by `AppHandle` for the Storage Manager table.
/// Each row carries a `ReleaseSummary` + its parent `AlbumSummary` on
/// the wire (two slices worth of data in one call). The ingest closure
/// splits them into the `releaseSummaries` and `albumSummaries` slices;
/// the view renders by iterating `ids` (release ids) and resolving via
/// both slices at render time.
struct StoragePageSource: PageSource {
    let library: Library
    let sort: BridgeStorageSort
    let filter: BridgeStorageFilter

    func count() async throws -> Int {
        try Int(library.storageCount(filter))
    }

    func page(offset: Int, limit: Int) async throws -> [BridgeStorageRow] {
        let page = try library.storagePage(
            sort,
            filter,
            UInt64(offset),
            UInt64(limit)
        )
        return page.rows
    }
}

/// `BridgeStorageRow` already has a stable id (the release id); lift it
/// into `Identifiable` so `PaginatedList<BridgeStorageRow>` compiles.
extension BridgeStorageRow: Identifiable {
    public var id: String {
        release.id
    }
}

// MARK: - AlbumList

/// The library grid's paginated list. Rows are `BridgeAlbum` wire rows;
/// the ingest closure interns each one as an `AlbumSummary` in the
/// store. Views resolve slots by reading `libraryStore.albumSummaries[id]`.
typealias AlbumList = PaginatedList<BridgeAlbum>
typealias ComposerList = PaginatedList<BridgeComposerSummary>

// MARK: - StorageList

/// The storage manager's paginated list. Rows are `BridgeStorageRow`
/// wire rows (a `ReleaseSummary` plus its parent `AlbumSummary`); the
/// ingest closure interns both halves into the library slices. Views
/// render by iterating release ids and resolving via `releaseSummaries`
/// + `albumSummaries`.
typealias StorageList = PaginatedList<BridgeStorageRow>

extension AlbumList {
    /// Construct a pre-populated list for SwiftUI previews and tests.
    /// Sorts albums up front, interns each into `store`, then seeds
    /// `ids` / `totalCount` so the preview renders without waiting
    /// for an async round-trip.
    static func preview(
        albums: [BridgeAlbum],
        sort: [BridgeSortCriterion],
        store: LibraryStore
    )
        -> AlbumList
    {
        let sorted = sortAlbums(albums: albums, criteria: sort)
        for bridge in sorted {
            _ = store.internAlbumSummary(bridge)
        }
        let list = AlbumList(
            pageSource: AlbumPreviewPageSource(albums: sorted),
            ingest: { rows in
                for row in rows {
                    _ = store.internAlbumSummary(row)
                }
            },
            onError: { _ in },
        )
        list.preloadForPreview(ids: sorted.map(\.id))
        return list
    }
}

// MARK: - BridgeAlbum: Identifiable

/// `BridgeAlbum` already has an `id: String` — lift it into the
/// `Identifiable` protocol so `PaginatedList<BridgeAlbum>` compiles.
extension BridgeAlbum: Identifiable {}

// MARK: - LibraryStore

/// Normalized slice store for library entities. Holds one slice per
/// entity type, keyed by id. Views compose data across slices at render
/// time (e.g. `albumSummaries[releaseSummaries[id].albumId].title`).
///
/// ## Slices
///
/// - `albumSummaries` — `AlbumSummary` keyed by album id. Library grid,
///   storage rows all join against this.
/// - `releaseSummaries` — `ReleaseSummary` keyed by release id. Storage
///   manager rows, release pickers read from here. Identity-stable so
///   pin toggles and format changes re-render without list rebuilds.
/// - `releaseDetails` — `ReleaseDetail` keyed by release id. The album
///   detail view reads this when the user selects a release. Each
///   detail wraps the identity-stable `ReleaseSummary` that also lives
///   in the `releaseSummaries` slice; interning a detail interns its
///   summary.
///
/// ## Reducers write slices; lists are views
///
/// Event reducers (`handle*`) write one or more slices and nothing
/// else. Event payloads are self-contained: the reducer writes the
/// incoming data unconditionally, without reading other slices to
/// decide what to do. They never notify lists, because lists are
/// views over a query at a point in time, not subscribers. If a
/// user action could change a visible list's shape, the action
/// handler calls `list.invalidate()` at the mutation site.
@MainActor
@Observable
final class LibraryStore {
    // ── Entity storage ────────────────────────────────────────────────

    /// Album summaries. Read by the library grid and any row renderer
    /// that needs album metadata without the fat payload.
    ///
    /// ## Source of writes
    /// - `handleAlbumAdded` / `handleAlbumUpdated` (library events)
    /// - `internAlbumSummary` (called by list page-source ingest)
    private(set) var albumSummaries: [String: AlbumSummary] = [:]

    /// Release summaries — the slim projection every list over releases
    /// reads (storage manager, release pickers, etc.). Identity-stable
    /// so in-band mutations (pin toggle, size updates, format fix)
    /// re-render affected rows without list rebuilds.
    ///
    /// ## Composition
    /// `ReleaseDetail` in the `releaseDetails` slice wraps the identity-
    /// stable `ReleaseSummary` from this slice — they must stay in
    /// sync. `internReleaseDetail` interns both halves in one call.
    ///
    /// ## Source of writes
    /// - `handleAlbumAdded` / `handleAlbumUpdated` (library events)
    /// - `handleReleaseAdded` / `handleReleaseUpdated` (release events)
    /// - `internReleaseSummary` / `internReleaseDetail` (list ingest)
    private(set) var releaseSummaries: [String: ReleaseSummary] = [:]

    /// Fat release payload for the album detail view: tracks, files,
    /// gallery items. Populated when a user expands an album or selects
    /// a release. Replaced wholesale on update — consumers read fields
    /// off the struct value, not by identity.
    ///
    /// Each detail's `summary` points into the `releaseSummaries` slice,
    /// so content mutations on the summary (pin state, total size) are
    /// picked up through the shared `ReleaseSummary` instance.
    ///
    /// ## Source of writes
    /// - `handleAlbumAdded` / `handleAlbumUpdated` (library events)
    /// - `handleReleaseAdded` / `handleReleaseUpdated` (release events)
    /// - `internReleaseDetail` (album-detail ingest)
    /// - `loadReleaseDetail` / `reloadReleaseDetail` (on-demand loaders)
    private(set) var releaseDetails: [String: ReleaseDetail] = [:]
    private(set) var composerSummaries: [String: BridgeComposerSummary] = [:]

    /// Fires when the library's *shape* changes — anything that could
    /// alter the total row count or ordering of a library- or
    /// storage-scoped list (add, remove, or metadata update that might
    /// move a sort key). Views holding a `PaginatedList` subscribe and
    /// call `list.invalidate()` so import-, sync-, and edit-driven
    /// shape changes show up without the list needing to observe the
    /// store. See `PaginatedList.swift`'s "lists are views, not
    /// subscribers" rule for the shape-vs-content distinction this
    /// subject bridges.
    ///
    /// The payload is a typed `LibraryShapeChange` variant. Subscribers
    /// filter on what matters to their list:
    ///
    /// - Library grid: invalidate on album-level variants only.
    ///   Release-level changes don't move rows.
    /// - Storage grid: invalidate on every variant — album changes move
    ///   whole groups of rows, release changes move individual rows.
    @ObservationIgnored
    let libraryShapeSubject = PassthroughSubject<LibraryShapeChange, Never>()

    nonisolated init() {}

    // MARK: - Interning

    /// Upsert the canonical `AlbumSummary` for this bridge payload.
    /// Idempotent: on update the existing instance is mutated in place
    /// (identity preserved so callers holding a reference keep seeing
    /// the same object), on insert a fresh instance is stored.
    func internAlbumSummary(_ bridge: BridgeAlbum) -> AlbumSummary {
        if let existing = albumSummaries[bridge.id] {
            existing.update(from: bridge)
            return existing
        }
        let summary = AlbumSummary(from: bridge)
        albumSummaries[bridge.id] = summary
        return summary
    }

    @discardableResult
    func internComposerSummary(_ bridge: BridgeComposerSummary)
        -> BridgeComposerSummary
    {
        composerSummaries[bridge.artistId] = bridge
        return bridge
    }

    /// Upsert the canonical `ReleaseSummary` for this bridge payload.
    /// Identity-stable: if a summary with this id already exists its
    /// fields are updated in place and the existing instance is
    /// returned. Callers (storage list ingest, `internReleaseDetail`)
    /// can hold the returned reference without worrying about it being
    /// replaced out from under them.
    @discardableResult
    func internReleaseSummary(_ bridge: BridgeReleaseSummary) -> ReleaseSummary
    {
        if let existing = releaseSummaries[bridge.id] {
            existing.update(from: bridge)
            return existing
        }
        let summary = ReleaseSummary(from: bridge)
        releaseSummaries[bridge.id] = summary
        return summary
    }

    /// Upsert from the fat `BridgeRelease` wire shape. Used by event
    /// reducers that receive a full album payload and want to populate
    /// both the summary slice and the detail slice from one event.
    @discardableResult
    func internReleaseSummary(_ bridge: BridgeRelease) -> ReleaseSummary {
        if let existing = releaseSummaries[bridge.id] {
            existing.update(from: bridge)
            return existing
        }
        let summary = ReleaseSummary(from: bridge)
        releaseSummaries[bridge.id] = summary
        return summary
    }

    /// Upsert the canonical `ReleaseDetail` and its embedded
    /// `ReleaseSummary`. Writes two slices from one call so the summary
    /// and detail stay in sync:
    ///
    /// 1. intern (or update) the identity-stable `ReleaseSummary` in
    ///    the `releaseSummaries` slice
    /// 2. build a fresh `ReleaseDetail` that wraps the canonical summary
    /// 3. store the detail in the `releaseDetails` slice
    @discardableResult
    func internReleaseDetail(_ bridge: BridgeRelease) -> ReleaseDetail {
        let summary = internReleaseSummary(bridge)
        let detail = ReleaseDetail(summary: summary, bridge: bridge)
        releaseDetails[bridge.id] = detail
        return detail
    }

    // MARK: - Detail loading

    /// Load fat detail for one release from the bridge. Called when the
    /// album detail view opens and no detail is yet cached for the
    /// selected release, or when the user switches to a release whose
    /// detail hasn't been loaded yet. Populates both `releaseDetails`
    /// and `releaseSummaries` (via `internReleaseDetail`).
    ///
    /// Callers loop over the releases of the currently-selected album.
    /// When the user switches albums the outer `.task(id: albumId)` is
    /// cancelled; we check cancellation before writing so we don't
    /// populate `releaseDetails` for the album the user just left.
    func loadReleaseDetail(releaseId: String, library: Library) async {
        guard releaseDetails[releaseId] == nil else {
            return
        }

        do {
            let findReleaseDetail = library.findReleaseDetail
            let bridge =
                try await Task.detached {
                    try findReleaseDetail(releaseId)
                }
                .value
            if Task.isCancelled {
                return
            }
            guard let bridge else {
                logger.error("Release detail not found for \(releaseId)")
                return
            }
            _ = internReleaseDetail(bridge)
        }
        catch {
            logger.error(
                "Failed to load release detail for \(releaseId): \(error.localizedDescription)"
            )
        }
    }

    /// Force-reload detail for a release. Used after operations that
    /// mutate data outside the event flow (cover change, pin/unpin).
    /// Checks cancellation before writing so a cancelled reload
    /// (e.g. the view navigated away mid-fetch) doesn't populate
    /// `releaseDetails` for a release no longer in scope.
    func reloadReleaseDetail(releaseId: String, library: Library) async {
        do {
            let findReleaseDetail = library.findReleaseDetail
            let bridge =
                try await Task.detached {
                    try findReleaseDetail(releaseId)
                }
                .value
            if Task.isCancelled {
                return
            }
            guard let bridge else {
                logger.error("Release detail not found for \(releaseId)")
                return
            }
            _ = internReleaseDetail(bridge)
        }
        catch {
            logger.error(
                "Failed to reload release detail for \(releaseId): \(error.localizedDescription)"
            )
        }
    }

    // MARK: - Album event handlers

    /// Reducer target for `AlbumAdded`. Decomposes the fat event
    /// payload across every affected slice:
    ///
    /// - `albumSummaries` via `internAlbumSummary`
    /// - `releaseSummaries` + `releaseDetails` via `internReleaseDetail`
    ///   for each release on the payload
    ///
    /// Does not notify lists — if this event was caused by a local user
    /// action, the action handler at the call site is responsible for
    /// calling `invalidate()` on any list that might need to reflect the
    /// shape change.
    func handleAlbumAdded(album: BridgeAlbumDetail) {
        _ = internAlbumSummary(album.album)

        for release in album.releases {
            _ = internReleaseDetail(release)
        }
    }

    /// Reducer target for `AlbumUpdated`. Same slice decomposition as
    /// `handleAlbumAdded` — the event shape is identical, so the
    /// reducers write the same slices. Writes are unconditional: if
    /// no prior summary exists (re-subscribe, out-of-order delivery)
    /// the reducer still interns from the payload, which upserts the
    /// missing entry. Field changes on identity-stable instances
    /// (`AlbumSummary`, `ReleaseSummary`) propagate through
    /// `@Observable` to views already reading them in place; the fat
    /// `releaseDetails` structs are replaced wholesale. Sort positions
    /// in live lists do not update here; mutators that could have
    /// changed the sort key call `list.invalidate()` at the action site.
    func handleAlbumUpdated(album: BridgeAlbumDetail) {
        _ = internAlbumSummary(album.album)

        for release in album.releases {
            _ = internReleaseDetail(release)
        }
    }

    /// Reducer target for `AlbumRemoved`. Drops the album from
    /// `albumSummaries` and cascades to the child releases the event
    /// carries, in both `releaseSummaries` and `releaseDetails`.
    ///
    /// Action handlers at the delete call site are responsible for
    /// invalidating any visible lists.
    func handleAlbumRemoved(albumId: String, releaseIds: [String]) {
        albumSummaries.removeValue(forKey: albumId)
        composerSummaries.removeAll()
        for id in releaseIds {
            releaseSummaries.removeValue(forKey: id)
            releaseDetails.removeValue(forKey: id)
        }
    }

    // MARK: - Release event handlers

    /// Reducer target for `ReleaseAdded`. Interns the release into both
    /// `releaseSummaries` and `releaseDetails`, and upserts the parent
    /// album so its `releaseIds` reflects the authoritative DB-ordered
    /// list carried in the event.
    func handleReleaseAdded(album: BridgeAlbum, release: BridgeRelease) {
        _ = internReleaseDetail(release)
        _ = internAlbumSummary(album)
        composerSummaries.removeAll()
    }

    /// Reducer target for `ReleaseUpdated`. Same slice writes as
    /// `handleReleaseAdded` — `internReleaseDetail` is identity-stable
    /// for the summary and replaces the detail wholesale.
    func handleReleaseUpdated(release: BridgeRelease) {
        _ = internReleaseDetail(release)
        composerSummaries.removeAll()
    }

    /// Reducer target for `ReleaseRemoved`. Drops the release from both
    /// `releaseSummaries` and `releaseDetails`, then interns the parent
    /// album's post-removal summary carried in the event — the same
    /// `internAlbumSummary` `handleReleaseAdded` uses — so `releaseIds`
    /// reflects the authoritative DB-ordered list without reading the old
    /// summary to patch it. `album` is nil when the album was removed with
    /// its last release; `AlbumRemoved` already dropped the summary then,
    /// so there's nothing further to do here.
    func handleReleaseRemoved(
        releaseId: String,
        album: BridgeAlbum?
    ) {
        releaseSummaries.removeValue(forKey: releaseId)
        releaseDetails.removeValue(forKey: releaseId)
        composerSummaries.removeAll()
        if let album {
            _ = internAlbumSummary(album)
        }
    }

    /// Reducer target for `ReleaseTransferProgress`. Sets the live transfer
    /// indicator on the identity-stable summary so the album-detail sheet and
    /// the Storage Manager row both reflect it. A no-op when the summary isn't
    /// loaded (the row isn't visible, so nothing renders the indicator).
    func handleReleaseTransferProgress(
        releaseId: String,
        label: String
    ) {
        releaseSummaries[releaseId]?.transfer = TransferState(
            label: label
        )
    }

    /// Reducer target for `ReleaseTransferEnded`. Clears the transfer indicator;
    /// any failure reason arrives separately via the thrown error path.
    func handleReleaseTransferEnded(releaseId: String) {
        releaseSummaries[releaseId]?.transfer = nil
    }
}
