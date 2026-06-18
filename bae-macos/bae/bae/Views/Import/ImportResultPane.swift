import SwiftUI

/// Search/results filling the available space, with the result detail docked
/// into a drag-resizable bottom pane that slides up when `open`. The caller
/// supplies the two regions and decides when the dock is open (a pressing was
/// picked, or its detail is loading); the pane owns its own resize height.
struct ImportResultPane<Top: View, Pane: View>: View {
    let open: Bool
    let onClose: () -> Void
    @ViewBuilder
    var top: () -> Top
    @ViewBuilder
    var pane: () -> Pane

    @State
    private var height: CGFloat = 384
    @State
    private var dragging = false

    var body: some View {
        GeometryReader { geo in
            let docked =
                open
                ? ImportPaneLayout.clamp(height, available: geo.size.height)
                : 0
            VStack(spacing: 0) {
                top()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                ImportResultBottomPane(
                    height: $height,
                    dragging: $dragging,
                    available: geo.size.height,
                    onClose: onClose,
                ) {
                    pane()
                }
                .frame(height: docked)
                .clipped()
                .animation(
                    dragging ? nil : .easeOut(duration: 0.28),
                    value: docked
                )
            }
        }
    }
}

// MARK: - Previews

#Preview("Confirming — results + docked detail") {
    @Previewable
    @State
    var values = rawReleaseEditFromUserEdit(
        edit: shapeUserEditFromReleaseDetail(
            detail: PreviewData.releaseDetailBridge,
            choice: .exact(
                releaseId: PreviewData.releaseDetailBridge.releaseId,
                source: PreviewData.releaseDetailBridge.source,
            )
        ),
        trackIdPrefix: "import-track"
    )
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationView(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            importDisabled: false,
            trackCountMismatch: PreviewData.releaseDetail.trackCountMismatch,
            expectedTrackCount: PreviewData.releaseDetail.trackCount,
            libraryStatus: nil,
            importStatus: nil,
            error: nil,
            hasCoverOptions: false,
            importing: false,
            exactness: ImportExactnessChoice(
                isMetadataOnly: false,
                onSelect: { _ in }
            ),
            onConfirmImport: {},
            onViewInLibrary: { _ in },
            onEditCover: {},
            coverContent: {
                ZStack {
                    Theme.placeholder
                    Image(systemName: "photo")
                        .foregroundStyle(.tertiary)
                }
                .frame(width: 80, height: 80)
                .clipShape(RoundedRectangle(cornerRadius: 6))
            },
            actionExtra: EmptyView.init,
        )
    }
    .frame(width: 700, height: 600)
    .background(Theme.background)
    .preferredColorScheme(.dark)
    .environment(MediaPaths.stub)
    .environment(UiStore())
    .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    .environment(
        ConfigStore(
            config: Config(
                bridge: BridgeConfig(
                    libraryId: "lib-preview",
                    libraryName: "Preview Library",
                    libraryPath: "/preview",
                    encryptionKeyStored: false,
                    encryptionKeyFingerprint: nil,
                    discogsTokenStatus: .notConfigured,
                    discogsUsable: false,
                    sync: nil
                )
            ),
            syncReady: false
        )
    )
}
