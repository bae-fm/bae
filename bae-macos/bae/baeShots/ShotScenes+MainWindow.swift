import BaeKit
import SwiftUI

@testable import bae

extension ShotScenes {
    /// The full main window against stub services over `backing`'s
    /// library, `needsAttention` as the Activity button shows it, or
    /// `syncActivity` when given (an idle one shows no button). The
    /// point of top padding keeps `ignoresSafeArea` from sliding the bar
    /// under the canvas's fake chrome.
    static func libraryWindow(
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        needsAttention: Bool = true,
        syncActivity: SyncActivityStore? = nil,
        playbackStore: PlaybackStore = PlaybackStore(),
        castStore: CastStore = CastStore(),
        importListSlot: ImportListSlot? = nil,
        projections: LibraryProjectionStore? = nil,
        size: CGSize = CGSize(width: 1280, height: 800)
    ) -> some View {
        let importScene = PreviewData.importTabScene()
        return MacMainWindowContainer()
            .environment(backing.library)
            .environment(backing.session)
            .environment(
                projections
                    ?? LibraryProjectionStore(library: backing.library)
            )
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
            .environment(castStore)
            .environment(
                syncActivity
                    ?? SyncActivityStore.preview(needsAttention: needsAttention)
            )
            .environment(SubsonicStore())
            .environment(\.libraryLoadError, nil)
            .environment(playbackStore)
            // The boards draw the Cast button, which the app shows once
            // casting is on. Set before the album preview's own default,
            // since the environment nearest the content wins.
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    castEnabled: true
                )
            )
            .albumDetailPreviewEnvironment()
            .mainWindowPreviewEnvironment()
            .padding(.top, 1)
            .frame(width: size.width, height: size.height)
            .windowBackground()
    }
}
