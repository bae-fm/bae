import BaeKit
import SwiftUI

// MARK: - Navigation types

enum MainSection {
    case library
    case importing
}

enum LibraryBrowserMode: CaseIterable {
    case albums
    case artists
    case composers

    var displayName: String {
        switch self {
        case .albums: String(localized: "Albums")
        case .artists: String(localized: "Artists")
        case .composers: String(localized: "Composers")
        }
    }
}

/// A pending "scroll the grid to this album" command, consumed exactly once by
/// `AlbumGridView`. Durable state, not a `PassthroughSubject`: the section that
/// mounts the grid (the library section can be unmounted entirely, or the
/// browser can be in composers mode) may not exist yet when the producer
/// emits. As state, the request persists until a mounted grid consumes it via
/// `UiStore.consumeAlbumReveal(seq:)`, which clears it so a later remount finds
/// nothing to replay. `seq` lets a consumer detect a new request even when
/// `albumId` repeats (navigate to the same album twice in a row) and drives
/// `.task(id:)` so SwiftUI restarts the reveal when a newer one arrives.
struct PendingAlbumReveal {
    let albumId: String
    let seq: Int
}

/// A pending "flash this track" command, consumed exactly once by the
/// `TrackRowView` whose `trackId` matches. Independent of `PendingAlbumReveal`
/// even though both are produced together by `navigateToAlbum` — the grid
/// scroll and the track flash are two different consumers with two different
/// lifetimes (the flash's track row may mount well after the grid scroll
/// completes), so one `take` must not starve the other. Durable and
/// seq-versioned for the same reasons as `PendingAlbumReveal`.
struct PendingTrackFlash {
    let trackId: String
    let seq: Int
}

enum LibraryNavigationTarget {
    case composer(String)
    case work(String)
}

/// A pending composer/work reveal, consumed exactly once by `LibraryView`.
/// Durable for the same reason as `PendingAlbumReveal`: cross-section
/// navigation can switch the browser mode and mount the consumer after the
/// producer records the request. `seq` lets repeat navigation to the same
/// target run again.
struct LibraryNavigationRequest {
    let target: LibraryNavigationTarget
    let seq: Int
}

// MARK: - UiStore

/// Shared UI-originated state. Views read properties, call methods to mutate.
/// Views never write fields directly. Core never produces this data.
@Observable
class UiStore: @unchecked Sendable {
    // ── Navigation ─────────────────────────────────────────────────────

    var activeSection: MainSection = .library
    var libraryBrowserMode: LibraryBrowserMode = .albums
    var selectedAlbumId: String?
    var showQueue: Bool = false

    /// The pending grid-scroll command, or `nil` before any navigation or once
    /// applied. Durable until consumed — see `PendingAlbumReveal`.
    private(set) var pendingAlbumReveal: PendingAlbumReveal?
    /// The pending track-flash command, or `nil` before any navigation, once
    /// applied, or when the navigation carried no track. Durable until
    /// consumed — see `PendingTrackFlash`.
    private(set) var pendingTrackFlash: PendingTrackFlash?
    private var revealSeq = 0

    /// The pending composer/work reveal, or `nil` before any such navigation or
    /// once applied. Durable until consumed — see `LibraryNavigationRequest`.
    private(set) var pendingLibraryNavigation: LibraryNavigationRequest?
    private var libraryNavigationSeq = 0

    // ── Shared selections ───────────────────────────────────────────────

    /// Release selected within a given album. Entries exist only when the user
    /// deviates from the default (first release). Missing key == default.
    var selectedReleaseIdByAlbum: [String: String] = [:]

    // ── Import candidate selection ──────────────────────────────────────

    /// The selected row in the import candidate list, or `nil` before any
    /// selection. UI-originated session state — which folder the user is
    /// looking at, not anything core produces — so it survives an import-tab
    /// remount instead of resetting to "Select a folder".
    var selectedFolderCandidate: String?

    /// The candidate list sidebar's active tab, filter text, and collapsed
    /// folders. UI-originated session state, alongside
    /// `selectedFolderCandidate` — surviving a remount so the sidebar doesn't
    /// reset to its defaults on every import-tab switch.
    var importCandidateTab: CandidateTab = .new
    var importCandidateFilterText: String = ""
    var collapsedImportFolders: Set<String> = []

    // ── Overlays ────────────────────────────────────────────────────────

    var lightbox: Cursor<LightboxItem>?
    private(set) var modalBuilder: (() -> AnyView)?

    // ── Errors ──────────────────────────────────────────────────────────

    var lastError: DisplayError?

    // ── Search ─────────────────────────────────────────────────────────

    var showSearchPopover: Bool = false

    /// Title-bar search popover results. Populated by the title bar's
    /// debounced query task and cleared on dismissal. Lives here because
    /// search is a UI-session concern: the user types into a transient
    /// popover and the result lives only as long as that popover does.
    var searchResults: SearchResults?

    // MARK: - Navigation methods

    func navigateToLibraryRoot() {
        activeSection = .library
    }

    func setLibraryBrowserMode(_ mode: LibraryBrowserMode) {
        libraryBrowserMode = mode
    }

    func navigateToAlbum(
        _ albumId: String,
        trackId: String? = nil,
        releaseId: String? = nil
    ) {
        activeSection = .library
        libraryBrowserMode = .albums
        selectedAlbumId = albumId
        if let releaseId {
            selectRelease(releaseId, inAlbum: albumId)
        }
        revealSeq += 1
        pendingAlbumReveal = PendingAlbumReveal(
            albumId: albumId,
            seq: revealSeq
        )
        // Always overwrite (to `nil` when there's no track) rather than leaving
        // a prior navigation's flash around unconsumed if its track row never
        // mounted.
        pendingTrackFlash = trackId.map {
            PendingTrackFlash(trackId: $0, seq: revealSeq)
        }
    }

    func navigateToImport() {
        activeSection = .importing
    }

    func navigateToComposer(_ artistId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSeq += 1
        pendingLibraryNavigation = LibraryNavigationRequest(
            target: .composer(artistId),
            seq: libraryNavigationSeq
        )
    }

    func navigateToWork(_ workId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSeq += 1
        pendingLibraryNavigation = LibraryNavigationRequest(
            target: .work(workId),
            seq: libraryNavigationSeq
        )
    }

    /// Clear the pending grid-scroll command once `AlbumGridView` has applied
    /// it. A no-op if a newer request already superseded it (that request owns
    /// the clear now).
    func consumeAlbumReveal(seq: Int) {
        if pendingAlbumReveal?.seq == seq {
            pendingAlbumReveal = nil
        }
    }

    /// Clear the pending track-flash command once the matching `TrackRowView`
    /// has applied it. A no-op if a newer request already superseded it.
    func consumeTrackFlash(seq: Int) {
        if pendingTrackFlash?.seq == seq {
            pendingTrackFlash = nil
        }
    }

    /// Clear the pending composer/work reveal once `LibraryView` has applied
    /// it. A no-op if a newer request already superseded it.
    func consumeLibraryNavigation(seq: Int) {
        if pendingLibraryNavigation?.seq == seq {
            pendingLibraryNavigation = nil
        }
    }

    func selectAlbum(_ albumId: String) {
        selectedAlbumId = albumId
    }

    func selectAlbumFromGrid(_ albumId: String?) {
        selectedAlbumId = albumId
    }

    func closeAlbumDetail() {
        selectedAlbumId = nil
    }

    func selectRelease(_ releaseId: String, inAlbum albumId: String) {
        selectedReleaseIdByAlbum[albumId] = releaseId
    }

    func clearSelectedRelease(inAlbum albumId: String) {
        selectedReleaseIdByAlbum.removeValue(forKey: albumId)
    }

    func switchSection(_ section: MainSection) {
        activeSection = section
    }

    func toggleQueue() {
        showQueue.toggle()
    }

    // MARK: - Import candidate selection methods

    func selectFolderCandidate(_ key: String?) {
        selectedFolderCandidate = key
    }

    func setImportCandidateTab(_ tab: CandidateTab) {
        importCandidateTab = tab
    }

    func setImportCandidateFilterText(_ text: String) {
        importCandidateFilterText = text
    }

    func toggleImportFolderCollapsed(_ path: String) {
        if collapsedImportFolders.contains(path) {
            collapsedImportFolders.remove(path)
        }
        else {
            collapsedImportFolders.insert(path)
        }
    }

    // MARK: - Error methods

    /// Surface a UI-originated error — prose the UI already localized (a failed
    /// drop, a caught Swift error). Core errors crossing the bridge use the
    /// typed overload.
    func showError(_ message: String) {
        lastError = DisplayError(line: message)
    }

    func showError(_ error: DisplayError) {
        lastError = error
    }

    func clearError() {
        lastError = nil
    }

    // MARK: - Overlay methods

    func presentModal(@ViewBuilder content: @escaping () -> some View) {
        modalBuilder = { AnyView(content()) }
    }

    func presentLightbox(items: [LightboxItem], preferring id: String? = nil) {
        lightbox = Cursor(items: items, preferring: id)
    }

    func dismissModal() {
        modalBuilder = nil
    }
}
