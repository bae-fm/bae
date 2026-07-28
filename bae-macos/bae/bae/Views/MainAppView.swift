import BaeKit
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
            // Main layout: title bar, active section + docked queue, now
            // playing bar
            VStack(spacing: 0) {
                TitleBar(searchText: $searchText)

                HStack(spacing: 0) {
                    // Only the active section is in the view tree: SwiftUI's
                    // .onHover is backed by AppKit tracking areas, which ignore
                    // both opacity and hit-testing, so a permanently-mounted
                    // inactive section leaks its hover chrome (popovers,
                    // tooltips) through to whichever section is showing.
                    Group {
                        if uiStore.activeSection == .library {
                            LibraryView()
                        }
                        else {
                            ImportView()
                        }
                    }
                    .frame(maxWidth: .infinity)

                    // Queue — docked as a sidebar, so the content reflows
                    // beside it and nothing is ever occluded while dragging
                    // albums or tracks into its drop sites. Toggled by the
                    // queue button or the menu; no click-away.
                    if uiStore.showQueue {
                        QueuePanel(
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
                    path: preview.path,
                    isPlaying: preview.isPlaying,
                    durationMs: preview.durationMs
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

                    // Anchored by its top edge 5pt under the field, trailing
                    // edges aligned, so the card can be content-sized and grow
                    // downward. Dismissal is the event monitor below, not a
                    // click-swallowing scrim: an outside click closes the
                    // dropdown AND still lands on its target.
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
                    .padding(.leading, rect.maxX - 572)
                    .padding(.top, rect.maxY + 5)
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
        .errorAlert(uiStore)
        .toolbar(.hidden)
        .ignoresSafeArea(.all, edges: .top)
        .modifier(TrafficLightOffset(xOffset: 6, yOffset: 7))
        .onDrop(of: [.fileURL], isTargeted: nil, perform: handleDrop)
        // A running import-audio preview is scoped to this library session, so
        // it ends when this view leaves the tree — closing, switching, or
        // locking the library. Not a tab switch: MainAppView stays mounted
        // across those; only its active child section unmounts. (The preview
        // overlay is a sibling here, not inside the section, so switching tabs
        // must NOT stop it.)
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

    // MARK: - Scan + Drop

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
            var isDir: ObjCBool = false
            guard
                FileManager.default.fileExists(
                    atPath: url.path,
                    isDirectory: &isDir
                ),
                isDir.boolValue
            else {
                DispatchQueue.main.async {
                    uiStore.showError(
                        String(localized: "Drop a folder to import, not a file")
                    )
                }
                return
            }
            DispatchQueue.main.async {
                Task {
                    do {
                        try await importer.addWatchedFolder(url.path)
                    }
                    catch {
                        uiStore.showError(
                            String(
                                localized:
                                    "Couldn't add folder: \(error.displayLine)"
                            )
                        )
                    }
                    uiStore.navigateToImport()
                }
            }
        }
        return true
    }
}

#if DEBUG
    /// The full main window against stub services: title bar, library grid
    /// over the canned 40-album session, and the now-playing bar with an
    /// empty transport. Scrolling the grid drives the header collapse; the
    /// queue and import sections mount on their normal toggles.
    ///
    /// The canvas's simulated window adds fake chrome with a top safe-area
    /// inset, and the view's own `ignoresSafeArea` (correct in the real
    /// chromeless window) slides the top bar underneath that chrome when the
    /// view sits flush against the safe-area boundary. One point of top
    /// padding keeps it off the boundary — the ignore has nothing adjacent
    /// to extend into, and the bar stays visible.
    #Preview("Main app") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewGridBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return MainAppView()
            .environment(library)
            .environment(session)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(PreviewAudio.stub)
            .environment(Cast.stub)
            .environment(CastStore())
            .albumDetailPreviewEnvironment(store: libraryStore)
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
