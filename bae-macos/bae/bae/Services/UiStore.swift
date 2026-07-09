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

/// A request to reveal an album (scroll the grid to it) and optionally flash
/// a track inside it. Durable state, not a `PassthroughSubject`: the consumer
/// (`AlbumGridView`, `AlbumDetailView`) does not exist when the producer emits.
/// Navigating from composers mode switches to albums mode, which *mounts* the
/// grid — so a subject would publish before any subscriber is alive and the
/// request would be lost. As state, the request persists across that remount
/// and the freshly-mounted grid reads it. `seq` is the version: it lets a
/// consumer detect a new request even when `albumId` repeats (navigate to the
/// same album twice), and drives `.task(id:)` so SwiftUI cancels a stale reveal
/// when a newer one arrives.
struct AlbumReveal {
    let albumId: String
    let trackId: String?
    let seq: Int
}

enum LibraryNavigationTarget {
    case composer(String)
    case work(String)
}

/// A request to reveal a composer or work in the composers browser. Durable for
/// the same reason as `AlbumReveal`: cross-section navigation can switch the
/// browser mode and mount the consumer after the producer records the request.
/// `seq` lets repeat navigation to the same target run again.
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

    /// The current album-reveal request (scroll + optional track flash), or
    /// `nil` before any navigation. Durable — see `AlbumReveal` for why this is
    /// state rather than a one-shot subject.
    private(set) var albumReveal: AlbumReveal?
    private var revealSeq = 0

    /// The current composer/work reveal request, or `nil` before any such
    /// navigation. Durable — see `LibraryNavigationRequest`.
    private(set) var libraryNavigationRequest: LibraryNavigationRequest?
    private var libraryNavigationSeq = 0

    // ── Shared selections ───────────────────────────────────────────────

    var selectedFolderCandidate: String?

    /// Release selected within a given album. Entries exist only when the user
    /// deviates from the default (first release). Missing key == default.
    var selectedReleaseIdByAlbum: [String: String] = [:]

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
        albumReveal = AlbumReveal(
            albumId: albumId,
            trackId: trackId,
            seq: revealSeq
        )
    }

    func navigateToImport() {
        activeSection = .importing
    }

    func navigateToComposer(_ artistId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSeq += 1
        libraryNavigationRequest = LibraryNavigationRequest(
            target: .composer(artistId),
            seq: libraryNavigationSeq
        )
    }

    func navigateToWork(_ workId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSeq += 1
        libraryNavigationRequest = LibraryNavigationRequest(
            target: .work(workId),
            seq: libraryNavigationSeq
        )
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
