import BaeKit
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
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — track-count warning") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            trackCountMismatch: true,
            expectedTrackCount: 11
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — metadata only") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            metadataOnly: true
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — importing") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            importStatus: .importing(
                progressPercent: 45,
                step: .running(phase: .measuringLoudness)
            ),
            importing: true
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — commit error") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            error: "Couldn't start import: invalid metadata."
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Loading detail") {
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ProgressView("Loading release details...")
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Conflict — dock closed") {
    ImportResultPane(open: false, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateConflict)
    } pane: {
        EmptyView()
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — already in library") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            libraryStatus: LibraryStatus(
                bridge: BridgeLibraryStatus(
                    releaseId: "rel-123",
                    releaseInLibrary: true,
                    albumInLibrary: true,
                    albumTitle: "Album Title",
                    albumId: "album-preview"
                )
            )
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}

#Preview("Confirming — cover options") {
    @Previewable
    @State
    var values = PreviewData.confirmEditValues
    @Previewable
    @State
    var storageManaged = true
    @Previewable
    @State
    var storagePinned = true
    ImportResultPane(open: true, onClose: {}) {
        ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
    } pane: {
        ImportConfirmationPreview.make(
            values: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            hasCoverOptions: true
        )
    }
    .frame(width: 1212, height: 982)
    .importPreviewEnvironment()
}
