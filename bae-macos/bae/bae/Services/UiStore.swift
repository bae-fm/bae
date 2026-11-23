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

    var displayName: String {
        switch self {
        case .albums: String(localized: "Albums")
        case .artists: String(localized: "Artists")
        case .composers: String(localized: "Composers")
        }
    }
}

/// The tabs of the side pane docked beside the library: the queue, the
/// playlists and the plays.
enum SidePaneTab: CaseIterable {
    case queue
    case playlists
    case history

    var title: String {
        switch self {
        case .queue: String(localized: "Queue")
        case .playlists: String(localized: "Playlists")
        case .history: String(localized: "History")
        }
    }
}

/// What a navigation asks the library browser to open.
enum LibraryNavigationTarget {
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
    /// Whether the side pane is docked beside the content.
    private(set) var showSidePane: Bool = false
    /// The side pane's tab, which it keeps while it is closed.
    private(set) var sidePaneTab: SidePaneTab = .queue
    /// The playlist open in the side pane's Playlists tab, which the tab
    /// shows again whenever the person comes back to it; `nil` shows the
    /// list of playlists.
    private(set) var openPlaylistId: String?
    /// Whose plays History shows: a person's member id, or `nil` for
    /// everyone's. History keeps the choice while the pane shows another
    /// tab.
    private(set) var historyPerson: String?

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
    private(set) var modalStack: [(CGSize) -> AnyView] = []
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

    /// Open the New playlist sheet, the playlist starting with the picked
    /// tracks `pickedTrackIds`, which core puts in their release's order.
    func presentNewPlaylist(pickedTrackIds: [String]) {
        newPlaylistSheet = NewPlaylistSheetState(pickedTrackIds: pickedTrackIds)
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

    func setSidePanePresented(_ presented: Bool) {
        showSidePane = presented
    }

    func toggleSidePane() {
        showSidePane.toggle()
    }

    /// Show the side pane on `tab`.
    func openSidePane(_ tab: SidePaneTab) {
        sidePaneTab = tab
        showSidePane = true
    }

    func selectSidePaneTab(_ tab: SidePaneTab) {
        sidePaneTab = tab
    }

    /// Open the playlist `playlistId`'s page in the side pane, on its
    /// Playlists tab.
    func openPlaylist(_ playlistId: String) {
        openPlaylistId = playlistId
        openSidePane(.playlists)
    }

    /// Show the plays of the person with member id `person` in History, or
    /// everyone's for `nil`.
    func showHistory(of person: String?) {
        historyPerson = person
    }

    /// Go back from the open playlist to the list of playlists.
    func closePlaylist() {
        openPlaylistId = nil
    }

    /// Go back to the list of playlists when the open playlist is
    /// `playlistId`, as when the person deletes it.
    func closePlaylist(_ playlistId: String) {
        if openPlaylistId == playlistId {
            openPlaylistId = nil
        }
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
        modalStack.append { _ in AnyView(content()) }
    }

    /// Open a modal over the ones already open, for a sheet that sizes itself
    /// to the window: `content` gets the window's size.
    func presentSizedModal(
        @ViewBuilder content: @escaping (CGSize) -> some View
    ) {
        modalStack.append { AnyView(content($0)) }
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
