import BaeKit
import SwiftUI

@testable import bae

extension ShotScenes {
    /// The full main window against stub services over `backing`'s
    /// library, `needsAttention` as the Activity button shows it. The
    /// point of top padding keeps `ignoresSafeArea` from sliding the bar
    /// under the canvas's fake chrome.
    static func libraryWindow(
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        needsAttention: Bool = true,
        playbackStore: PlaybackStore = PlaybackStore(),
        importListSlot: ImportListSlot? = nil,
        size: CGSize = CGSize(width: 1280, height: 800)
    ) -> some View {
        let importScene = PreviewData.importTabScene()
        return MacMainWindowContainer()
            .environment(backing.library)
            .environment(backing.session)
            .environment(LibraryProjectionStore(library: backing.library))
            .environment(uiStore)
            .environment(importScene.store)
            .environment(
                \.previewPosition,
                importScene.store.previewPosition.only
            )
            .environment(
                importListSlot ?? importScene.slot(uiStore: uiStore)
            )
            .environment(ImportSelection())
            .environment(SettingsNavigation())
            .environment(PreviewAudio.stub())
            .environment(Cast.stub())
            .environment(CastStart.stub())
            .environment(CastStore())
            .environment(
                SyncActivityStore.preview(needsAttention: needsAttention)
            )
            .environment(SubsonicStore())
            .environment(\.libraryLoadError, nil)
            .environment(playbackStore)
            .albumDetailPreviewEnvironment()
            .mainWindowPreviewEnvironment()
            .padding(.top, 1)
            .frame(width: size.width, height: size.height)
            .windowBackground()
    }
}
