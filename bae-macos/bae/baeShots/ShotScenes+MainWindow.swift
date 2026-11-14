import BaeKit
import SwiftUI

@testable import bae

extension ShotScenes {
    /// The full main window against stub services over `backing`'s
    /// library, `needsAttention` as the Activity button shows it. The
    /// point of top padding keeps `ignoresSafeArea` from sliding the bar
    /// under the canvas's fake chrome.
    ///
    /// The Import section reads `importScene`'s store and lists, with
    /// `importSelection` as the selected rows (the one `importScene`'s slot
    /// was made with, unless `importListSlot` stands in), what each
    /// candidate has in flight from `candidateRuntimes`, and its covers
    /// from `images`; `configStore`, `releaseEditor`, `outbox` and
    /// `importPercents` stand in for the ones a shot of the Import section
    /// states.
    static func libraryWindow(
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        needsAttention: Bool = true,
        playbackStore: PlaybackStore = PlaybackStore(),
        castStore: CastStore = CastStore(),
        importListSlot: ImportListSlot? = nil,
        importScene: ImportPreviewFixture = PreviewData.importTabScene(),
        importSelection: ImportSelection = ImportSelection(),
        candidateRuntimes: [Data: BridgeCandidateRuntimeSnapshot] = [:],
        images: ImageStore? = nil,
        configStore: ConfigStore? = nil,
        releaseEditor: ReleaseEditor? = nil,
        outbox: OutboxStore? = nil,
        importPercents: LatestStream<UInt32>? = nil,
        projections: LibraryProjectionStore? = nil,
        size: CGSize = CGSize(width: 1280, height: 800)
    ) -> some View {
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
                importListSlot
                    ?? importScene.slot(
                        uiStore: uiStore,
                        selection: importSelection
                    )
            )
            .environment(importSelection)
            .environment(\.heldCandidateRuntimes, candidateRuntimes)
            .environment(SettingsNavigation())
            .environment(PreviewAudio.stub())
            .environment(Cast.stub())
            .environment(CastStart.stub())
            .environment(castStore)
            .environment(
                SyncActivityStore.preview(needsAttention: needsAttention)
            )
            .environment(SubsonicStore())
            .environment(\.libraryLoadError, nil)
            .environment(playbackStore)
            .environmentIfGiven(images)
            .environmentIfGiven(configStore)
            .environmentIfGiven(releaseEditor)
            .environmentIfGiven(outbox)
            .environment(\.importPercents, importPercents)
            .albumDetailPreviewEnvironment()
            .mainWindowPreviewEnvironment()
            .padding(.top, 1)
            .frame(width: size.width, height: size.height)
            .windowBackground()
    }
}

extension View {
    /// `object` in the environment when a shot gives one; otherwise nothing
    /// is set here, so the values an enclosing view or the preview
    /// environment sets stay.
    @ViewBuilder
    fileprivate func environmentIfGiven<Object: AnyObject & Observable>(
        _ object: Object?
    ) -> some View {
        if let object {
            environment(object)
        }
        else {
            self
        }
    }
}
