#if DEBUG
import BaeKit
import SwiftUI

/// The named UI scenes captured for the cross-platform screenshot gallery.
/// Each builder composes production views over `PreviewData` fixtures; the
/// matching `#Preview`s render these same builders, so the gallery captures
/// and the previews render one path and can't drift.
@MainActor
enum PreviewScenes {
    /// The first-run onboarding entry screen over the app's dark background.
    static func welcome() -> some View {
        OnboardingEntryScreen(
            error: nil,
            onJoin: {},
            onScanRecovery: {},
            onPasteRecovery: {}
        )
        .background(Theme.background)
    }

    /// The album grid backed by the fixture albums, over a seeded store and the
    /// shared service stubs — the same composition the grid's `#Preview` shows.
    static func libraryGrid() -> some View {
        let store = PreviewData.libraryStore()
        return AlbumGrid(
            list: AlbumList.preview(albums: PreviewData.albums, store: store),
            onSelect: { _ in }
        )
        .background(Theme.background)
        .previewStores(libraryStore: store)
    }

    /// The album detail for the seeded fixture album: cover, metadata, the
    /// transport actions, and the track list. Nothing is playing, so the
    /// now-playing bar's bottom safe-area inset stays empty and the content
    /// scroll view fills the frame reliably offscreen.
    static func albumDetail() -> some View {
        AlbumDetailView(albumId: "a-1")
            .previewStores(
                playbackStore: PreviewData.playbackStore(nowPlaying: false)
            )
    }
}
#endif
