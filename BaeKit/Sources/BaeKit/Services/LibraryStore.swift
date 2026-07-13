import Foundation
import Observation
import os.log

private let logger = Logger.bae("LibraryStore")

// MARK: - Album page sources

/// Page source backed by the `Library` domain service for the full
/// library grid.
public struct LibraryAlbumPageSource: PageSource {
    public let library: Library
    public let sort: [BridgeSortCriterion]

    public init(library: Library, sort: [BridgeSortCriterion]) {
        self.library = library
        self.sort = sort
    }

    public func count() async throws -> Int {
        Int(try await library.getAlbumCount())
    }

    public func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        try await library.getAlbumPage(sort, UInt64(offset), UInt64(limit))
    }
}

/// In-memory page source for album previews and tests. Expects rows to
/// arrive pre-sorted; serves contiguous slices.
public struct AlbumPreviewPageSource: PageSource {
    public let albums: [BridgeAlbum]

    public init(albums: [BridgeAlbum]) {
        self.albums = albums
    }

    public func count() async throws -> Int {
        albums.count
    }

    public func page(offset: Int, limit: Int) async throws -> [BridgeAlbum] {
        let start = min(offset, albums.count)
        let end = min(start + limit, albums.count)
        return Array(albums[start..<end])
    }
}

public struct LibraryComposerPageSource: PageSource {
    public let library: Library
    public let sort: [BridgeComposerSortCriterion]

    public init(library: Library, sort: [BridgeComposerSortCriterion]) {
        self.library = library
        self.sort = sort
    }

    public func count() async throws -> Int {
        Int(try await library.getComposerCount())
    }

    public func page(offset: Int, limit: Int) async throws
        -> [BridgeComposerSummary]
    {
        try await library.getComposerPage(sort, UInt64(offset), UInt64(limit))
    }
}

extension BridgeComposerSummary: Identifiable {
    public var id: String { artistId }
}

public struct LibraryArtistPageSource: PageSource {
    public let library: Library
    public let sort: [BridgeArtistSortCriterion]

    public init(library: Library, sort: [BridgeArtistSortCriterion]) {
        self.library = library
        self.sort = sort
    }

    public func count() async throws -> Int {
        Int(try await library.getArtistCount())
    }

    public func page(offset: Int, limit: Int) async throws
        -> [BridgeArtistSummary]
    {
        try await library.getArtistPage(sort, UInt64(offset), UInt64(limit))
    }
}

extension BridgeArtistSummary: Identifiable {
    public var id: String { artistId }
}

// MARK: - Storage page source

/// Page source backed by `AppHandle` for the Storage Manager table.
/// Each row carries a `ReleaseSummary` + its parent `AlbumSummary` on
/// the wire (two slices worth of data in one call). The ingest closure
/// splits them into the `releaseSummaries` and `albumSummaries` slices;
/// the view renders by iterating `ids` (release ids) and resolving via
/// both slices at render time.
public struct StoragePageSource: PageSource {
    public let library: Library
    public let sort: BridgeStorageSort
    public let filter: BridgeStorageFilter

    public init(
        library: Library,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter
    ) {
        self.library = library
        self.sort = sort
        self.filter = filter
    }

    public func count() async throws -> Int {
        Int(try await library.storageCount(filter))
    }

    public func page(offset: Int, limit: Int) async throws -> [BridgeStorageRow]
    {
        let page = try await library.storagePage(
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
public typealias AlbumList = PaginatedList<BridgeAlbum>
public typealias ComposerList = PaginatedList<BridgeComposerSummary>
public typealias ArtistList = PaginatedList<BridgeArtistSummary>

// MARK: - StorageList

/// The storage manager's paginated list. Rows are `BridgeStorageRow`
/// wire rows (a `ReleaseSummary` plus its parent `AlbumSummary`); the
/// ingest closure interns both halves into the library slices. Views
/// render by iterating release ids and resolving via `releaseSummaries`
/// + `albumSummaries`.
public typealias StorageList = PaginatedList<BridgeStorageRow>

extension AlbumList {
    /// Construct a pre-populated list for SwiftUI previews and tests.
    /// Interns each album into `store`, then seeds `ids` / `totalCount` so the
    /// preview renders without waiting for an async round-trip.
    ///
    /// Rows render in the order given. The live grid is ordered by the database,
    /// through the `ORDER BY` its page query builds; a preview holds a hand-written
    /// list with no database, so it presents the order its author wrote.
    public static func preview(
        albums: [BridgeAlbum],
        store: LibraryStore
    )
        -> AlbumList
    {
        for bridge in albums {
            _ = store.internAlbumSummary(bridge)
        }
        let list = AlbumList(
            pageSource: AlbumPreviewPageSource(albums: albums),
            ingest: { rows in
                for row in rows {
                    _ = store.internAlbumSummary(row)
                }
            },
            onError: { _ in },
        )
        list.preloadForPreview(ids: albums.map(\.id))
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
/// ## Queries write slices; lists are projections
///
/// Page-source ingest and release-detail projections write one or more slices
/// from core query results. Scoped invalidations refresh visible lists through
/// `ProjectionRegistry`; the store does not fold library event deltas.
@MainActor
@Observable
public final class LibraryStore {
    // ── Entity storage ────────────────────────────────────────────────

    /// Album summaries. Read by the library grid and any row renderer
    /// that needs album metadata without the fat payload.
    ///
    /// Written by `internAlbumSummary` from list page-source ingest.
    public private(set) var albumSummaries: [String: AlbumSummary] = [:]

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
    /// Written by `internReleaseSummary` / `internReleaseDetail`.
    public private(set) var releaseSummaries: [String: ReleaseSummary] = [:]

    /// Fat release payload for the album detail view: tracks, files,
    /// gallery items. Populated when a user expands an album or selects
    /// a release. Replaced wholesale on update — consumers read fields
    /// off the struct value, not by identity.
    ///
    /// Each detail's `summary` points into the `releaseSummaries` slice,
    /// so content mutations on the summary (pin state, total size) are
    /// picked up through the shared `ReleaseSummary` instance.
    ///
    /// Written by `internReleaseDetail` and on-demand detail loaders.
    public private(set) var releaseDetails: [String: ReleaseDetail] = [:]
    public private(set) var composerSummaries: [String: BridgeComposerSummary] =
        [:]
    public private(set) var artistSummaries: [String: BridgeArtistSummary] =
        [:]

    /// Per-release detail-load failures, keyed by release id. `loadReleaseDetail`
    /// records the failure here instead of swallowing it, so the album detail
    /// view can show an error + Retry rather than spinning on the placeholder.
    /// Cleared when a load for that release starts again or succeeds.
    public private(set) var releaseDetailErrors: [String: DisplayError] = [:]

    /// Total albums in the open library, or `nil` before any album list has
    /// reported a count. Written by the macOS library browser whenever its
    /// album list's count loads or changes; read by menu chrome that has no
    /// list of its own (the Playback menu's Shuffle Library item disables at
    /// zero). Albums are the proxy the UI owns for "anything to play": core's
    /// shuffle no-ops on zero *tracks*, but the UI never loads a track count.
    public private(set) var albumTotal: Int?

    public nonisolated init() {}

    /// Record the library-wide album total reported by an album-list count
    /// load.
    public func setAlbumTotal(_ count: Int) {
        albumTotal = count
    }

    // MARK: - Interning

    /// Upsert the canonical `AlbumSummary` for this bridge payload.
    /// Idempotent: on update the existing instance is mutated in place
    /// (identity preserved so callers holding a reference keep seeing
    /// the same object), on insert a fresh instance is stored.
    public func internAlbumSummary(_ bridge: BridgeAlbum) -> AlbumSummary {
        if let existing = albumSummaries[bridge.id] {
            existing.update(from: bridge)
            return existing
        }
        let summary = AlbumSummary(from: bridge)
        albumSummaries[bridge.id] = summary
        return summary
    }

    @discardableResult
    public func internComposerSummary(_ bridge: BridgeComposerSummary)
        -> BridgeComposerSummary
    {
        composerSummaries[bridge.artistId] = bridge
        return bridge
    }

    @discardableResult
    public func internArtistSummary(_ bridge: BridgeArtistSummary)
        -> BridgeArtistSummary
    {
        artistSummaries[bridge.artistId] = bridge
        return bridge
    }

    /// Upsert the canonical `ReleaseSummary` for this bridge payload.
    /// Identity-stable: if a summary with this id already exists its
    /// fields are updated in place and the existing instance is
    /// returned. Callers (storage list ingest, `internReleaseDetail`)
    /// can hold the returned reference without worrying about it being
    /// replaced out from under them.
    @discardableResult
    public func internReleaseSummary(_ bridge: BridgeReleaseSummary)
        -> ReleaseSummary
    {
        if let existing = releaseSummaries[bridge.id] {
            existing.update(from: bridge)
            return existing
        }
        let summary = ReleaseSummary(from: bridge)
        releaseSummaries[bridge.id] = summary
        return summary
    }

    /// Upsert from the fat `BridgeRelease` wire shape — the summary half of
    /// `internReleaseDetail(_:)`'s two-slice write.
    @discardableResult
    public func internReleaseSummary(_ bridge: BridgeRelease) -> ReleaseSummary
    {
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
    public func internReleaseDetail(_ bridge: BridgeRelease) -> ReleaseDetail {
        let summary = internReleaseSummary(bridge)
        let detail = ReleaseDetail(summary: summary, bridge: bridge)
        releaseDetails[bridge.id] = detail
        return detail
    }

    public func internAlbumDetail(_ bridge: BridgeAlbumDetail) {
        _ = internAlbumSummary(bridge.album)
        for release in bridge.releases {
            _ = internReleaseDetail(release)
        }
    }

    public func applyReleaseDetailSnapshot(
        releaseId: String,
        bridge: BridgeRelease?
    ) {
        guard let bridge else {
            releaseSummaries.removeValue(forKey: releaseId)
            releaseDetails.removeValue(forKey: releaseId)
            composerSummaries.removeAll()
            artistSummaries.removeAll()
            return
        }
        _ = internReleaseDetail(bridge)
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
    public func loadReleaseDetail(releaseId: String, library: Library) async {
        guard releaseDetails[releaseId] == nil else {
            return
        }
        // Clear any prior failure for this release: this call is the retry.
        releaseDetailErrors[releaseId] = nil

        do {
            let findReleaseDetail = library.findReleaseDetail
            let bridge =
                try await Task.detached {
                    try await findReleaseDetail(releaseId)
                }
                .value
            if Task.isCancelled {
                return
            }
            guard let bridge else {
                releaseDetailErrors[releaseId] = DisplayError(
                    line: String(localized: "Release detail not found")
                )
                return
            }
            _ = internReleaseDetail(bridge)
        }
        catch {
            if Task.isCancelled {
                return
            }
            releaseDetailErrors[releaseId] = DisplayError(
                line: error.localizedDescription
            )
        }
    }

    /// Force-reload detail for a release. Used after operations that
    /// mutate data outside the event flow (cover change, pin/unpin).
    /// Checks cancellation before writing so a cancelled reload
    /// (e.g. the view navigated away mid-fetch) doesn't populate
    /// `releaseDetails` for a release no longer in scope.
    public func reloadReleaseDetail(releaseId: String, library: Library) async {
        do {
            let findReleaseDetail = library.findReleaseDetail
            let bridge =
                try await Task.detached {
                    try await findReleaseDetail(releaseId)
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

}
