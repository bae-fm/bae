#if DEBUG
    import SwiftUI

    /// Preview builder for ImportConfirmationView — fixes the cover placeholder,
    /// the EmptyView action extra, and the action callbacks, exposing only the
    /// display knobs a permutation varies. (ImportConfirmationView is generic
    /// over its cover/action content, so this returns `some View`.)
    enum ImportConfirmationPreview {
        static func make(
            values: Binding<BridgeRawReleaseEdit>,
            storageManaged: Binding<Bool>,
            storagePinned: Binding<Bool>,
            trackCountMismatch: Bool = false,
            expectedTrackCount: UInt32 = 9,
            libraryStatus: LibraryStatus? = nil,
            importStatus: ImportStatus? = nil,
            error: String? = nil,
            hasCoverOptions: Bool = false,
            importing: Bool = false,
            metadataOnly: Bool = false,
        ) -> some View {
            ImportConfirmationView(
                values: values,
                storageManaged: storageManaged,
                storagePinned: storagePinned,
                importDisabled: false,
                trackCountMismatch: trackCountMismatch,
                expectedTrackCount: expectedTrackCount,
                libraryStatus: libraryStatus,
                importStatus: importStatus,
                error: error,
                hasCoverOptions: hasCoverOptions,
                importing: importing,
                exactness: ImportExactnessChoice(
                    isMetadataOnly: metadataOnly,
                    onSelect: { _ in }
                ),
                onConfirmImport: {},
                onViewInLibrary: { _ in },
                onEditCover: {},
                coverContent: {
                    ZStack {
                        Theme.placeholder
                        Image(systemName: "photo").foregroundStyle(.tertiary)
                    }
                    .frame(width: 80, height: 80)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                },
                actionExtra: EmptyView.init
            )
        }
    }

    /// The stores every import preview reads plus the app's window background,
    /// injected as one modifier: MediaPaths + UiStore for the search/file panes,
    /// OutboxStore + ConfigStore for the confirmation, and `windowBackground()`
    /// so the preview reproduces the shell the panes are transparent over.
    extension View {
        func importPreviewEnvironment() -> some View {
            self
                .environment(MediaPaths.stub)
                .environment(UiStore())
                .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
                .environment(PreviewData.configStore)
                .windowBackground()
        }
    }
#endif
