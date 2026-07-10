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

    private var queueActions: QueueActions {
        QueueActions(library: library, queue: queue, uiStore: uiStore)
    }

    var body: some View {
        ZStack {
            // Main layout: title bar, active section, now playing bar
            VStack(spacing: 0) {
                TitleBar(searchText: $searchText)

                // Only the active section is in the view tree: SwiftUI's
                // .onHover is backed by AppKit tracking areas, which ignore
                // both opacity and hit-testing, so a permanently-mounted
                // inactive section leaks its hover chrome (popovers, tooltips)
                // through to whichever section is showing.
                if uiStore.activeSection == .library {
                    LibrarySection()
                }
                else {
                    ImportView()
                }
                Divider()
                NowPlayingBarContainer(
                    onQueueInsertTracks: { ids, index in
                        queueActions.insertInQueue(ids, at: index)
                    },
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

                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture {
                            uiStore.showSearchPopover = false
                            NSApp.keyWindow?.makeFirstResponder(nil)
                        }

                    SearchView(
                        results: uiStore.searchResults,
                        onSelectAlbum: selectAlbum,
                        onSelectComposer: selectComposer,
                        onSelectWork: selectWork,
                    )
                    .frame(width: 400, height: 350, alignment: .topTrailing)
                    .position(x: rect.maxX - 200, y: rect.maxY + 180)
                }
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
                do {
                    try importer.addWatchedFolder(url.path)
                }
                catch {
                    uiStore.showError(
                        String(
                            localized:
                                "Couldn't add folder: \(error.localizedDescription)"
                        )
                    )
                }
                uiStore.navigateToImport()
            }
        }
        return true
    }
}
