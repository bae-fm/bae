import BaeKit
import Combine
import SwiftUI
import UniformTypeIdentifiers

/// The space the search dropdown is placed in: the whole window's content,
/// where the title bar reports the search field's frame.
enum SearchOverlaySpace {
    static let name = "searchOverlay"
}

/// Reads the stores for `MacMainWindow` and holds what surrounds it: the
/// layers over it, the search dropdown, the file importer and the alerts.
struct MacMainWindowContainer: View {
    @Environment(Queue.self)
    var queue
    @Environment(Library.self)
    var library
    @Environment(Importer.self)
    var importer
    @Environment(ImportStore.self)
    var importStore
    @Environment(PreviewAudio.self)
    var previewAudio
    @Environment(UiStore.self)
    var uiStore
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(Playback.self)
    var playback
    @Environment(LibraryBrowseSession.self)
    var session
    @Environment(CastStart.self)
    var castStart
    @State
    private var searchText: String
    private let initialHoveredQueueEntry: String?
    /// The lightbox's image as it loads.
    @State
    private var lightboxLoad: LightboxImageLoad
    /// The dropdown card's frame in the search overlay's space, fed to the
    /// dismiss monitor so interactions inside the card don't close it.
    @State
    private var searchCardFrame: CGRect = .zero
    /// The search field's frame in `SearchOverlaySpace`, as the title bar
    /// reports it when its own layout moves the field. Read from the field
    /// alone rather than gathered as a preference over the whole window,
    /// which every layout below — each scrolled list row — combined again.
    @State
    private var searchFieldFrame: CGRect?

    /// The window with `searchText` typed in the search field, whose lightbox
    /// loads its image in `lightboxLoad`.
    init(
        searchText: String = "",
        initialHoveredQueueEntry: String? = nil,
        lightboxLoad: LightboxImageLoad = LightboxImageLoad()
    ) {
        _searchText = State(initialValue: searchText)
        _lightboxLoad = State(initialValue: lightboxLoad)
        self.initialHoveredQueueEntry = initialHoveredQueueEntry
    }

    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    /// What the window shows over its content, each over the one before: the
    /// folder lightbox, the audio preview, each modal, and the document
    /// viewer, over the modal it may have been opened from.
    private var layers: [MacLayer] {
        var layers: [MacLayer] = []
        if let lightbox = uiStore.lightbox {
            layers.append(
                MacLayer(id: "lightbox", fades: true) {
                    FolderLightboxLayer(
                        lightbox: lightbox,
                        load: lightboxLoad,
                        onDismiss: { uiStore.closeLightbox() }
                    )
                }
            )
        }
        if let preview = importStore.previewState.active {
            layers.append(
                MacLayer(id: "preview") {
                    PreviewOverlay(
                        name: preview.target.name,
                        isPlaying: preview.isPlaying
                    )
                }
            )
        }
        for (index, builder) in uiStore.modalStack.enumerated() {
            layers.append(
                MacLayer(id: "modal \(index)") {
                    MacModal(
                        onDismiss: { uiStore.dismissModal() },
                        sizedContent: builder
                    )
                }
            )
        }
        if let document = uiStore.document {
            layers.append(
                MacLayer(id: "document") {
                    MacModal(onDismiss: { uiStore.dismissDocument() }) {
                        DocumentViewerView(
                            name: document.name,
                            text: document.text,
                            onClose: { uiStore.dismissDocument() }
                        )
                    }
                }
            )
        }
        return layers
    }

    var body: some View {
        MacMainWindow(showSidePane: uiStore.showSidePane) {
            MacTitleBar(
                searchText: $searchText,
                onSearchFieldFrame: { searchFieldFrame = $0 }
            )
        } content: {
            // Only the active section is mounted: `.onHover` tracking
            // areas ignore opacity, so a hidden one would leak hover.
            if uiStore.activeSection == .library {
                LibraryView()
            }
            else {
                MacImportWindow(endEditing: endEditing)
            }
        } queuePanel: {
            MacQueuePanelContainer(
                initialHoveredEntry: initialHoveredQueueEntry,
                onClose: {
                    uiStore.setSidePanePresented(false)
                },
                onInsertTracks: { dropped, index in
                    queueActions.insertDropped(
                        dropped,
                        at: index,
                        order: AlbumGridOrder(session: session)
                    )
                }
            )
        } nowPlayingBar: {
            MacNowPlayingBarContainer(
                onDropToQueue: { dropped in
                    queueActions.addDropped(
                        dropped,
                        order: AlbumGridOrder(session: session)
                    )
                },
            )
        }
        .macLayers(layers)
        .overlay {
            if uiStore.showSearchPopover, uiStore.searchResults != nil,
                let rect = searchFieldFrame
            {
                // Hangs under the title bar, 4 points below it, with its
                // trailing edge on the field's; the monitor below dismisses it
                // and lets the click through.
                MacSearchCard(
                    results: uiStore.searchResults,
                    onSelectAlbum: selectAlbum,
                    onSelectTrack: selectTrack,
                    onSelectArtist: selectArtist,
                    onSelectComposer: selectComposer,
                    onSelectWork: selectWork,
                )
                .onGeometryChange(for: CGRect.self) { geo in
                    geo.frame(in: .named(SearchOverlaySpace.name))
                } action: {
                    searchCardFrame = $0
                }
                .padding(.leading, rect.maxX - MacSearchCard.width)
                .padding(
                    .top,
                    rect.midY + MacTitleBar.height / 2 + ThemeSpace.inline
                )
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .topLeading
                )
                .background(
                    SearchDismissMonitor(
                        insideRects: [searchCardFrame, rect],
                        onClickAway: {
                            uiStore.showSearchPopover = false
                            NSApp.keyWindow?.makeFirstResponder(nil)
                        },
                        onScrollAway: {
                            uiStore.showSearchPopover = false
                        }
                    )
                )
            }
        }
        .coordinateSpace(name: SearchOverlaySpace.name)
        // Clicking anywhere that is not a text field ends the active field
        // edit, in this window and in popovers alike.
        .background(FieldClickAwayMonitor())
        .opensWithoutFieldFocus()
        .playbackFailureAlert(
            playbackStore,
            pairAgain: { castStart.cast(to: $0) }
        ) {
            let playback = playback
            Task {
                do { try await playback.dismissFailure() }
                catch { uiStore.showError(error) }
            }
        }
        .sheet(
            item: Binding(
                get: { castStart.codeSheet },
                set: { if $0 == nil { castStart.cancelCode() } }
            )
        ) { sheet in
            CastCodeSheet(
                sheet: sheet,
                onPair: { castStart.pair(code: $0) },
                onCancel: { castStart.cancelCode() }
            )
        }
        .castRefusalAlert(castStart)
        .newPlaylistSheet()
        .fileImporter(
            isPresented: Binding(
                get: { uiStore.isImportFolderPickerPresented },
                set: { uiStore.setImportFolderPickerPresented($0) }
            ),
            allowedContentTypes: [.directory],
            onCompletion: { importFolderEntry.take($0) }
        )
        .fileDialogDefaultDirectory(.homeDirectory)
        .fileDialogMessage("Select a folder to watch for music to import")
        .fileDialogConfirmationLabel("Add")
        .toolbar(.hidden)
        .ignoresSafeArea(.all, edges: .top)
        .onDrop(of: [.fileURL], isTargeted: nil) {
            importFolderEntry.take(dropped: $0)
        }
        .sidePausePromptAlert(showError: { uiStore.showError($0) })
        // An import-audio shot ends with the library session, not a tab.
        .onDisappear { previewAudio.previewStop() }
    }

    private func endEditing() {
        NSApp.keyWindow?.makeFirstResponder(nil)
    }

    // MARK: - Import entry

    private var importFolderEntry: ImportFolderEntry {
        ImportFolderEntry(importer: importer, uiStore: uiStore)
    }
}

/// The folder lightbox once core's read says where it stands. It reads that
/// itself, so a step through the images redraws only the lightbox's layer.
private struct FolderLightboxLayer: View {
    let lightbox: FolderImagesLightbox
    let load: LightboxImageLoad
    let onDismiss: () -> Void

    var body: some View {
        if let place = lightbox.place {
            MacLightbox(
                place: place,
                load: load,
                onSelect: { lightbox.focus($0) },
                onDismiss: onDismiss
            )
        }
    }
}

// MARK: - Search selection

extension MacMainWindowContainer {
    private func selectAlbum(_ albumId: String) {
        closeSearchPopover()
        uiStore.navigateToAlbum(albumId)
    }

    private func selectTrack(_ track: TrackSearchResult) {
        closeSearchPopover()
        uiStore.navigateToAlbum(
            track.albumId,
            trackId: track.id,
            releaseId: track.releaseId
        )
    }

    private func selectArtist(_ artistId: String) {
        closeSearchPopover()
        uiStore.navigateToArtist(artistId)
    }

    private func selectComposer(_ artistId: String) {
        closeSearchPopover()
        uiStore.navigateToComposer(artistId)
    }

    private func selectWork(_ workId: String) {
        closeSearchPopover()
        uiStore.navigateToWork(workId)
    }

    private func closeSearchPopover() {
        uiStore.showSearchPopover = false
        searchText = ""
        NSApp.keyWindow?.makeFirstResponder(nil)
        uiStore.searchResults = nil
    }
}
