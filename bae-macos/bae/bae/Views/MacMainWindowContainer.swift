import BaeKit
import Combine
import SwiftUI
import UniformTypeIdentifiers

/// The space the search dropdown is placed in: the whole window's content,
/// where the title bar reports the search field's frame.
enum SearchOverlaySpace {
    static let name = "searchOverlay"
}

/// The sort priority of the `rank`th part VoiceOver reads after the window's
/// parts and its side pane: below the side pane's default 0, and above the -2
/// of an open menu, which the boards list last.
private enum ReadingPriority {
    static func of(rank: Int) -> Double {
        -Double(rank) / 10
    }
}

/// Reads the stores for `MacMainWindow` and holds what surrounds it: the
/// overlays, the search dropdown, the file importer and the alerts.
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
    private var lightboxLoad = LightboxImageLoad()
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

    /// The window with `searchText` typed in the search field.
    init(
        searchText: String = "",
        initialHoveredQueueEntry: String? = nil
    ) {
        _searchText = State(initialValue: searchText)
        self.initialHoveredQueueEntry = initialHoveredQueueEntry
    }

    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    var body: some View {
        ZStack {
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
                // VoiceOver reads the bar after every modal and the document.
                // A modal's priority is below the side pane's, which keeps
                // the default.
                .accessibilityElement(children: .contain)
                .accessibilitySortPriority(
                    ReadingPriority.of(
                        rank: uiStore.modalStack.count
                            + (uiStore.document == nil ? 1 : 2)
                    )
                )
            }

            // Lightbox overlay
            if let lightbox = uiStore.lightbox, let place = lightbox.place {
                MacLightbox(
                    place: place,
                    load: lightboxLoad,
                    onSelect: { lightbox.focus($0) },
                    onDismiss: { uiStore.closeLightbox() },
                )
                .transition(.opacity)
                .zIndex(1)
            }

            // Audio shot overlay
            if let preview = importStore.previewState.active {
                PreviewOverlay(
                    name: preview.target.name,
                    isPlaying: preview.isPlaying
                )
            }

            // Modal overlay
            // VoiceOver reads each modal after the side pane and after the
            // modals under it, as the boards do: an earlier modal's priority
            // is the higher.
            ForEach(Array(uiStore.modalStack.enumerated()), id: \.offset) {
                index,
                builder in
                MacModal(
                    onDismiss: { uiStore.dismissModal() },
                    sizedContent: builder
                )
                .accessibilitySortPriority(
                    ReadingPriority.of(rank: index + 1)
                )
            }

            // Document viewer, over the modal it may have been opened from
            if let document = uiStore.document {
                MacModal(onDismiss: { uiStore.dismissDocument() }) {
                    DocumentViewerView(
                        name: document.name,
                        text: document.text,
                        onClose: { uiStore.dismissDocument() }
                    )
                }
                .accessibilitySortPriority(
                    ReadingPriority.of(rank: uiStore.modalStack.count + 1)
                )
            }
        }
        .animation(.easeInOut(duration: 0.2), value: uiStore.lightbox != nil)
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
        .modifier(TrafficLightOffset(xOffset: 6, yOffset: 7))
        .onDrop(of: [.fileURL], isTargeted: nil) {
            importFolderEntry.take(dropped: $0)
        }
        .sidePausePromptAlert(showError: { uiStore.showError($0) })
        // An import-audio shot ends with the library session, not a tab.
        .onDisappear { previewAudio.previewStop() }
    }

    // MARK: - Search selection

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

    private func endEditing() {
        NSApp.keyWindow?.makeFirstResponder(nil)
    }

    // MARK: - Import entry

    private var importFolderEntry: ImportFolderEntry {
        ImportFolderEntry(importer: importer, uiStore: uiStore)
    }
}
