import BaeKit
import Combine
import SwiftUI
import UniformTypeIdentifiers

struct SearchFieldAnchorKey: PreferenceKey {
    nonisolated(unsafe) static var defaultValue: Anchor<CGRect>?
    static func reduce(
        value: inout Anchor<CGRect>?,
        nextValue: () -> Anchor<CGRect>?
    ) {
        value = nextValue() ?? value
    }
}

struct MainAppView: View {
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
    @State
    private var searchText: String = ""
    /// The dropdown card's frame in the search overlay's space, fed to the
    /// dismiss monitor so interactions inside the card don't close it.
    @State
    private var searchCardFrame: CGRect = .zero

    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    var body: some View {
        ZStack {
            // Title bar, the active section beside the queue, now playing bar.
            VStack(spacing: 0) {
                TitleBar(searchText: $searchText)
                ArtworkLoadingBanner()

                HStack(spacing: 0) {
                    // Only the active section is mounted: `.onHover` tracking
                    // areas ignore opacity, so a hidden one would leak hover.
                    Group {
                        if uiStore.activeSection == .library {
                            LibraryView()
                        }
                        else {
                            ImportView(endEditing: endEditing)
                        }
                    }
                    .frame(maxWidth: .infinity)

                    // Docked so the content reflows beside it during drags.
                    if uiStore.showQueue {
                        QueuePanel(
                            onClose: {
                                uiStore.setQueuePresented(false)
                            },
                            onInsertTracks: { ids, index in
                                queueActions.insertInQueue(ids, at: index)
                            }
                        )
                        .transition(.move(edge: .trailing))
                    }
                }
                .animation(
                    .spring(duration: 0.24, bounce: 0.12),
                    value: uiStore.showQueue
                )

                Divider()
                NowPlayingBarContainer(
                    onDropToQueue: { ids in queueActions.addToQueue(ids) },
                )
            }

            // Lightbox overlay
            if let cursor = uiStore.lightbox {
                LightboxView(
                    cursor: cursor,
                    onUpdate: { uiStore.lightbox = $0 },
                    onDismiss: { uiStore.lightbox = nil },
                )
                .transition(.opacity)
                .zIndex(1)
            }

            // Audio preview overlay
            if let preview = importStore.previewState.active {
                PreviewOverlay(
                    path: preview.target.path,
                    isPlaying: preview.isPlaying
                )
            }

            // Modal overlay
            if let builder = uiStore.modalBuilder {
                ModalOverlay(onDismiss: { uiStore.dismissModal() }) {
                    builder()
                }
            }
        }
        .animation(.easeInOut(duration: 0.2), value: uiStore.lightbox != nil)
        .overlayPreferenceValue(SearchFieldAnchorKey.self) { anchor in
            if uiStore.showSearchPopover, uiStore.searchResults != nil,
                let anchor
            {
                GeometryReader { proxy in
                    let rect = proxy[anchor]

                    // Hangs under the field, trailing edges aligned; the
                    // monitor below dismisses it and lets the click through.
                    SearchView(
                        results: uiStore.searchResults,
                        onSelectAlbum: selectAlbum,
                        onSelectArtist: selectArtist,
                        onSelectComposer: selectComposer,
                        onSelectWork: selectWork,
                    )
                    .onGeometryChange(for: CGRect.self) { geo in
                        geo.frame(in: .named("searchOverlay"))
                    } action: {
                        searchCardFrame = $0
                    }
                    .padding(.leading, rect.maxX - SearchView.width)
                    .padding(.top, rect.maxY + ThemeSpace.inline)
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
                .coordinateSpace(name: "searchOverlay")
            }
        }
        // Clicking anywhere that is not a text field ends the active field
        // edit, in this window and in popovers alike.
        .background(FieldClickAwayMonitor())
        .errorAlert(uiStore)
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
        .onDrop(of: [.fileURL], isTargeted: nil, perform: handleDrop)
        .sidePausePromptAlert(showError: { uiStore.showError($0) })
        // An import-audio preview ends with the library session, not a tab.
        .onDisappear { previewAudio.previewStop() }
    }

    // MARK: - Search selection

    private func selectAlbum(_ albumId: String) {
        closeSearchPopover()
        uiStore.selectAlbum(albumId)
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

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else {
            return false
        }
        guard
            provider.hasItemConformingToTypeIdentifier(
                UTType.fileURL.identifier
            )
        else {
            return false
        }
        provider.loadItem(
            forTypeIdentifier: UTType.fileURL.identifier,
            options: nil
        ) { data, _ in
            guard let data = data as? Data,
                let url = URL(dataRepresentation: data, relativeTo: nil)
            else {
                DispatchQueue.main.async {
                    uiStore.showError(
                        String(localized: "Could not read dropped item")
                    )
                }
                return
            }
            DispatchQueue.main.async {
                importFolderEntry.take(url)
            }
        }
        return true
    }
}

#if DEBUG
    /// The main window against stub services. The point of top padding keeps
    /// `ignoresSafeArea` from sliding the bar under the canvas's fake chrome.
    #Preview("Main app") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewGridBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        let libraryProjections = LibraryProjectionStore(library: library)
        let importScene = PreviewData.importTabScene()
        return MainAppView()
            .environment(library)
            .environment(session)
            .environment(libraryStore)
            .environment(libraryProjections)
            .environment(uiStore)
            .environment(importScene.store)
            .environment(importScene.slot(uiStore: uiStore))
            .environment(ImportSelection())
            .environment(PreviewAudio.stub())
            .environment(Cast.stub())
            .environment(CastStore())
            .environment(ArtworkLoadingStore(cancel: {}))
            .albumDetailPreviewEnvironment(store: libraryStore)
            .environment(
                \.playbackPositionPublisher,
                Empty<PlaybackPositionEvent, Never>().eraseToAnyPublisher()
            )
            .environment(
                \.previewProgressPublisher,
                Empty<PlaybackPositionEvent, Never>().eraseToAnyPublisher()
            )
            .padding(.top, 1)
            .frame(width: 1280, height: 800)
            .windowBackground()
    }

    /// The empty library's whole window (desktop story 3) — the same
    /// composition the shot harness renders.
    #Preview("Main app \u{2014} Empty") {
        PreviewScenes.libraryEmpty()
    }
#endif
