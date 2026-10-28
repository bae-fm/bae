import BaeKit
import SwiftUI

/// The selected release's transfers and files, read for it in core, and the
/// details of the file selected among them. Detail observation lives here so
/// list rows depend only on their summary projection.
struct StorageContentsInspector: View {
    @Environment(Library.self)
    private var library

    let releaseId: String

    /// The inspector's one read, moved as `releaseId` changes.
    @State
    private var reader: DetailReader<BridgeStorageInspector>?
    /// The release as last read; `nil` until it is, and while the library
    /// holds no such release. Core answers a new release within a frame or
    /// two, so the pane stays empty until it does rather than flashing a
    /// spinner.
    @State
    private var inspector: BridgeStorageInspector?
    @State
    private var error: DisplayError?
    /// The file whose details show under the list; `nil` while none is
    /// selected, and again once the inspector moves to another release.
    @State
    private var selectedFileId: String?

    var body: some View {
        Group {
            if let error {
                LoadFailureView(error: error) {
                    reader?.retry()
                }
            }
            else if let inspector {
                VStack(spacing: 0) {
                    StorageTransferControls(items: inspector.transfers)
                    fileList(inspector.files)
                    if let selected = inspector.files.first(where: {
                        $0.file.id == selectedFileId
                    }) {
                        Divider()
                        StorageFileDetails(file: selected.file) {
                            selectedFileId = nil
                        }
                    }
                }
            }
            else {
                PendingReadPane(selection: releaseId)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear { showRelease(releaseId) }
        .onChange(of: releaseId) { _, newId in
            selectedFileId = nil
            showRelease(newId)
        }
        .onDisappear { reader?.close() }
    }

    private func fileList(_ rows: [BridgeStorageInspectorFile]) -> some View {
        Group {
            if rows.isEmpty {
                ContentUnavailableView { Text("No files") }
            }
            else {
                List(rows, id: \.file.id, selection: $selectedFileId) { row in
                    StorageInspectorFileRow(row: row)
                }
                .listStyle(.plain)
                .accessibilityIdentifier("storage-inspector-files")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func showRelease(_ releaseId: String) {
        // Another release's value shows nothing of this one.
        if releaseId != reader?.id {
            inspector = nil
            error = nil
        }
        let reader =
            reader
            ?? DetailReader(
                open: library.storageInspector,
                onValue: { _, value in
                    inspector = value
                    error = nil
                },
                onError: { _, error in self.error = DisplayError(error) }
            )
        self.reader = reader
        reader.show(releaseId)
    }
}
