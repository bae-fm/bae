#if DEBUG
    import BaeKit
    import SwiftUI

    /// The named UI scenes captured for the cross-platform screenshot gallery.
    /// Each builder composes production views over `PreviewData` fixtures; the
    /// matching `#Preview`s render these same builders, so the gallery captures
    /// and the previews render one path and can't drift.
    @MainActor
    enum PreviewScenes {
        /// First-run welcome: the chooser landing screen in the welcome window's
        /// fixed 900x600 chrome, over the populated `LibrarySetup` fixture.
        static func welcome() -> some View {
            WelcomeWindowChrome {
                WelcomeView(onLibraryReady: { _ in })
            }
            .environment(PreviewData.welcomeSetup)
        }

        /// The restore-from-cloud flow, entered directly, in the welcome chrome.
        static func welcomeRestore() -> some View {
            WelcomeWindowChrome {
                WelcomeView(onLibraryReady: { _ in }, initialMode: .restore)
            }
            .environment(PreviewData.welcomeSetup)
        }

        /// The album grid backed by the fixture albums, wired with the same live
        /// stores and stub services the app injects at the main window root.
        static func libraryGrid() -> some View {
            let uiStore = UiStore()
            let libraryStore = LibraryStore()
            let backing = LibraryView.previewGridBacking(
                uiStore: uiStore,
                libraryStore: libraryStore
            )
            return LibraryView()
                .environment(MediaPaths.stub)
                .environment(Playback.stub)
                .environment(Queue.stub)
                .environment(Downloads.stub)
                .environment(backing.library)
                .environment(libraryStore)
                .environment(uiStore)
                .environment(backing.session)
                .environment(PreviewData.configStore)
                .windowBackground()
        }

        /// The full main window over a library with nothing in it — title bar,
        /// empty albums state, idle now-playing bar (desktop story 3). Same
        /// composition as `MainAppView`'s preview, over the shared empty
        /// backing (`LibraryView.previewEmptyBacking`).
        static func libraryEmpty() -> some View {
            let uiStore = UiStore()
            let libraryStore = LibraryStore()
            let backing = LibraryView.previewEmptyBacking(
                uiStore: uiStore,
                libraryStore: libraryStore
            )
            return MainAppView()
                .environment(backing.library)
                .environment(backing.session)
                .environment(libraryStore)
                .environment(uiStore)
                .environment(PreviewAudio.stub)
                .environment(Cast.stub)
                .environment(CastStore())
                .albumDetailPreviewEnvironment(store: libraryStore)
                .windowBackground()
        }

        /// The expanded album detail for a two-sided vinyl fixture, mid-playback.
        static func albumDetail() -> some View {
            PreviewData.albumExpansionScene(
                albumId: "a-21",
                currentTrackId: "t-d2-3",
                isPlaying: true
            )
        }
    }
#endif
