import Combine
import SwiftUI

// MARK: - Navigation types

enum MainSection {
    case library
    case importing
}

enum LibraryBrowserMode: CaseIterable {
    case albums
    case composers

    var displayName: String {
        switch self {
        case .albums: String(localized: "Albums")
        case .composers: String(localized: "Composers")
        }
    }
}

/// One-shot imperative command fired when the UI should navigate to and
/// reveal an album (and optionally flash a track inside it).
struct NavigationCommand {
    let albumId: String
    let trackId: String?
}

enum LibraryNavigationTarget {
    case composer(String)
    case work(String)
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

    /// One-shot navigation commands (scroll/flash). State fields describe
    /// what the UI *is*; this subject carries what should *happen once*.
    @ObservationIgnored
    let navigationSubject = PassthroughSubject<NavigationCommand, Never>()

    @ObservationIgnored
    let libraryNavigationSubject = PassthroughSubject<
        LibraryNavigationTarget, Never
    >()

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
        navigationSubject.send(
            NavigationCommand(albumId: albumId, trackId: trackId)
        )
    }

    func navigateToImport() {
        activeSection = .importing
    }

    func navigateToComposer(_ artistId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSubject.send(.composer(artistId))
    }

    func navigateToWork(_ workId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigationSubject.send(.work(workId))
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

    func clearSelectedReleaseIfMatching(
        _ releaseId: String,
        inAlbum albumId: String
    ) {
        if selectedReleaseIdByAlbum[albumId] == releaseId {
            selectedReleaseIdByAlbum.removeValue(forKey: albumId)
        }
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
