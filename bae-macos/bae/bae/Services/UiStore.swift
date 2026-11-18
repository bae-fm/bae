import BaeKit
import Combine
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
    case playlists

    var displayName: String {
        switch self {
        case .albums: String(localized: "Albums")
        case .artists: String(localized: "Artists")
        case .composers: String(localized: "Composers")
        case .playlists: String(localized: "Playlists")
        }
    }
}

/// What a navigation asks the library browser to open.
enum LibraryNavigationTarget: Equatable {
    case artist(String)
    case composer(String)
    case work(String)
}

/// A request to show an album in the album grid, by a navigation such as Go
/// to Now Playing rather than a click on its card, and the track in it whose
/// row to show, if any.
struct AlbumReveal: Equatable, Sendable {
    let albumId: String
    /// The track whose row the grid scrolls into view and then flashes, or
    /// `nil` for the album alone.
    let trackId: String?
}

// MARK: - UiStore

/// Shared UI-originated state. Views read properties, call methods to mutate.
/// Views never write fields directly. Core never produces this data.
@Observable
class UiStore: @unchecked Sendable {
    // ── Navigation ─────────────────────────────────────────────────────

    var activeSection: MainSection = .library
    var libraryBrowserMode: LibraryBrowserMode = .albums
    /// The New playlist sheet's state while it is open.
    var newPlaylistSheet: NewPlaylistSheetState?
    private(set) var selectedAlbumId: String?
    /// The tracks picked on the open album's page.
    let pickedTracks = PickedTracks()
    private(set) var showQueue: Bool = false

    /// The requests to show an album in the album grid, which the grid takes
    /// as they come.
    @ObservationIgnored
    let albumReveals = HeldRequests<AlbumReveal>()
    /// The track whose row flashes, sent once the grid has shown the row a
    /// reveal names, so the flash is seen where the row is.
    @ObservationIgnored
    let trackFlashes = PassthroughSubject<String, Never>()

    /// The requests to open an artist, a composer or a work in the library
    /// browser, which the browse session takes as they come.
    @ObservationIgnored
    let libraryNavigations = HeldRequests<LibraryNavigationTarget>()

    // ── Shared selections ───────────────────────────────────────────────

    /// Release selected within a given album: one the user picked other than
    /// the album's primary, or the one a navigation named. Missing key means
    /// the primary release.
    var selectedReleaseIdByAlbum: [String: String] = [:]

    // ── Import candidate list ───────────────────────────────────────────

    /// The requests to show an import candidate in the candidate list, by
    /// its key, which the list takes as they come.
    @ObservationIgnored
    let importCandidateReveals = HeldRequests<Data>()

    /// The candidate list sidebar's active tab and filters. UI-originated
    /// session state — surviving a remount so the sidebar doesn't reset to its
    /// defaults on every import-tab switch, and gone at relaunch, where a
    /// filter still hiding rows would read as rows gone missing.
    var importCandidateTab: BridgeTriageTab = .pending
    var importCandidateFilterText: String = ""
    var importCandidatePendingFilter: BridgePendingFilter = .all

    private(set) var refreshingWatchedFolders: Set<Data> = []

    // ── Find online notices ─────────────────────────────────────────────

    /// Whether the Discogs-not-configured bar has been put away. What it
    /// states is about the app, not any one candidate, so putting it away in
    /// one candidate's pane holds for every pane. Never persisted: a new
    /// launch says it again.
    private(set) var discogsNoticeDismissed = false

    // ── Overlays ────────────────────────────────────────────────────────

    /// The lightbox over a candidate's folder images, while it is open.
    private(set) var lightbox: FolderImagesLightbox?
    /// The open modals, the first presented at the bottom. The window draws
    /// each modal over the one before it, which stays open beneath: the
    /// release editor's Change cover opens over the editor.
    private(set) var modalStack: [() -> AnyView] = []
    /// The document open in the viewer. It has a layer of its own, over the
    /// modal: a document opened from the release editor's modal leaves the
    /// editor open beneath it.
    private(set) var document: OpenDocument?
    private(set) var isImportFolderPickerPresented = false

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

    /// Open the New playlist sheet, the playlist starting with `trackIds`.
    func presentNewPlaylist(trackIds: [String]) {
        newPlaylistSheet = NewPlaylistSheetState(trackIds: trackIds)
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
        albumReveals.send(AlbumReveal(albumId: albumId, trackId: trackId))
    }

    func navigateToImport() {
        activeSection = .importing
    }

    /// Go to the import tab with `candidateKey` selected alone, scrolled to
    /// wherever the list places it.
    func navigateToImportCandidate(_ candidateKey: Data) {
        activeSection = .importing
        importCandidateReveals.send(candidateKey)
    }

    func navigateToArtist(_ artistId: String) {
        activeSection = .library
        libraryBrowserMode = .artists
        libraryNavigations.send(.artist(artistId))
    }

    func navigateToComposer(_ artistId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigations.send(.composer(artistId))
    }

    func navigateToWork(_ workId: String) {
        activeSection = .library
        libraryBrowserMode = .composers
        libraryNavigations.send(.work(workId))
    }

    /// Flash `trackId`'s row, once the grid has shown it.
    func flashTrack(_ trackId: String) {
        trackFlashes.send(trackId)
    }

    func selectAlbum(_ albumId: String?) {
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

    func setQueuePresented(_ presented: Bool) {
        showQueue = presented
    }

    func setImportCandidateTab(_ tab: BridgeTriageTab) {
        importCandidateTab = tab
    }

    func setImportCandidateFilterText(_ text: String) {
        importCandidateFilterText = text
    }

    func setImportCandidatePendingFilter(_ filter: BridgePendingFilter) {
        importCandidatePendingFilter = filter
    }

    func setWatchedFolderRefreshing(_ path: Data, _ refreshing: Bool) {
        if refreshing {
            refreshingWatchedFolders.insert(path)
        }
        else {
            refreshingWatchedFolders.remove(path)
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

    /// Surface a caught error. An error core says has no line — a cancellation —
    /// is dropped rather than raised as an alert with nothing in it.
    func showError(_ error: any Error) {
        guard let displayed = DisplayError(error) else { return }
        lastError = displayed
    }

    func clearError() {
        lastError = nil
    }

    // MARK: - Overlay methods

    /// Open a modal over the ones already open.
    func presentModal(@ViewBuilder content: @escaping () -> some View) {
        modalStack.append { AnyView(content()) }
    }

    func presentDocument(name: String, text: String) {
        document = OpenDocument(name: name, text: text)
    }

    /// Read the document `name` with `read` and show it in the viewer; a
    /// read that fails shows on the app's alert, naming the file.
    @MainActor
    func openDocument(
        name: String,
        read: @escaping @Sendable () async throws -> String
    ) {
        Task {
            do {
                presentDocument(name: name, text: try await read())
            }
            catch {
                // No line means a cancellation, which raises no alert.
                if let failure = DisplayError(error)?
                    .rewording({
                        String(localized: "Could not read \(name): \($0)")
                    })
                {
                    showError(failure)
                }
            }
        }
    }

    func dismissDocument() {
        document = nil
    }

    @MainActor
    func presentLightbox(_ lightbox: FolderImagesLightbox) {
        self.lightbox?.close()
        self.lightbox = lightbox
    }

    @MainActor
    func closeLightbox() {
        lightbox?.close()
        lightbox = nil
    }

    /// Close the modal on top; the ones beneath stay open.
    func dismissModal() {
        _ = modalStack.popLast()
    }

    /// Close every open modal.
    func dismissAllModals() {
        modalStack.removeAll()
    }

    func setImportFolderPickerPresented(_ presented: Bool) {
        isImportFolderPickerPresented = presented
    }

    // MARK: - Find online notice methods

    func dismissDiscogsNotice() {
        discogsNoticeDismissed = true
    }
}

/// A document open in the viewer: the file's name and its text.
struct OpenDocument: Equatable {
    let name: String
    let text: String
}
