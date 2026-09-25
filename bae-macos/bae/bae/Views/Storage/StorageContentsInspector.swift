import BaeKit
import SwiftUI

/// The files belonging to the selected release. Detail observation lives here
/// so list rows depend only on their summary projection.
struct StorageContentsInspector: View {
    @Environment(Library.self)
    private var library
    @Environment(LibraryStore.self)
    private var libraryStore

    @Environment(OutboxStore.self)
    private var outboxStore

    let releaseId: String

    /// The inspector's one release read, moved as `releaseId` changes.
    @State
    private var detailReader: DetailReader<BridgeRelease>?

    var body: some View {
        VStack(spacing: 0) {
            StorageTransferControls(releaseId: releaseId)
            fileList
        }
        .onAppear { showRelease(releaseId) }
        .onChange(of: releaseId) { _, newId in showRelease(newId) }
        .onDisappear { detailReader?.close() }
    }

    private var fileList: some View {
        Group {
            if let error = libraryStore.releaseDetailErrors[releaseId] {
                LoadFailureView(line: error.line) {
                    detailReader?.retry()
                }
            }
            else if let detail = libraryStore.releaseDetails[releaseId] {
                let rows = bridgeStorageInspectorFiles(
                    releaseId: releaseId,
                    files: detail.files,
                    outbox: outboxStore.snapshot
                )
                if rows.isEmpty {
                    ContentUnavailableView("No files", systemImage: "doc")
                }
                else {
                    List(rows, id: \.identity) { row in
                        StorageInspectorFileRow(row: row)
                    }
                    .listStyle(.plain)
                    .accessibilityIdentifier("storage-inspector-files")
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func showRelease(_ releaseId: String) {
        let reader =
            detailReader ?? libraryStore.releaseDetailReader(library: library)
        detailReader = reader
        reader.show(releaseId)
    }
}
