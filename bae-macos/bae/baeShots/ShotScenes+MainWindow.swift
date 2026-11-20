import BaeKit
import SwiftUI

@testable import bae

extension ShotScenes {
    /// Builds the full main window against stub services over `backing`'s
    /// library. The Activity button shows `syncActivity` when the caller
    /// passes one (an idle one shows no button), and `needsAttention`
    /// otherwise. The
    /// point of top padding keeps `ignoresSafeArea` from sliding the bar
    /// under the canvas's fake chrome.
    ///
    /// The Import section reads `importScene`'s store and lists, with
    /// `importSelection` as the selected rows (unless `importListSlot`
    /// stands in, with its own), what each candidate has in flight from
    /// `candidateRuntimes`, and its covers from `images`; `configStore`, `releaseEditor`, `outbox`
    /// and `importPercents` stand in for the ones a shot of the Import
    /// section states.
    static func libraryWindow(
        uiStore: UiStore,
        backing: (library: Library, session: LibraryBrowseSession),
        needsAttention: Bool = true,
        syncActivity: SyncActivityStore? = nil,
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
        history: HistoryStore? = nil,
        syncStatus: SyncStatusStore = PreviewData.syncStatus(
            hasCloudHome: true
        ),
        size: CGSize = CGSize(width: 1280, height: 800)
    ) -> some View {
        let slot =
            importListSlot
            ?? importScene.slot(uiStore: uiStore, selection: importSelection)
        return MacMainWindowContainer()
            .environment(backing.library)
            .environment(backing.session)
            .environment(
                projections
                    ?? LibraryProjectionStore(library: backing.library)
            )
            .environment(uiStore)
            .importSectionEnvironment(
                store: importScene.store,
                slot: slot,
                runtimes: candidateRuntimes
            )
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
                configStore
                    ?? PreviewData.makeConfigStore(
                        libraryFullWidth: false,
                        castEnabled: true
                    )
            )
            .environment(
                history
                    ?? HistoryStore(library: backing.library, onError: { _ in })
            )
            // Ana, an admin, whose picture ends the title bar.
            .environment(
                MembershipStore.stub(
                    people: PreviewData.members(
                        reaching: PreviewData.previewDropboxStorage
                    ),
                    selfIsAdmin: true
                )
            )
            .environment(syncStatus)
            .environmentIfGiven(images)
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
    /// What the Import section reads: its store, the list's slot and the
    /// selection in it, the position of the preview that plays, and what each
    /// candidate has in flight.
    func importSectionEnvironment(
        store: ImportStore,
        slot: ImportListSlot,
        runtimes: [Data: BridgeCandidateRuntimeSnapshot]
    ) -> some View {
        self
            .environment(store)
            .environment(\.previewPosition, store.previewPosition.only)
            .environment(slot)
            .environment(slot.selection)
            .environment(\.heldCandidateRuntimes, runtimes)
    }

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
