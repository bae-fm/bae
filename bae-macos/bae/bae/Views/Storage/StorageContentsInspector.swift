import BaeKit
import SwiftUI

/// The selected release's transfers and files, read for it in core, and the
/// details of the file selected among them, read for that file by its id.
/// Detail observation lives here so list rows depend only on their summary
/// projection.
struct StorageContentsInspector: View {
    @Environment(Library.self)
    private var library

    let releaseId: String

    init(releaseId: String, initialSelectedFileId: String? = nil) {
        self.releaseId = releaseId
        _selectedFileId = State(initialValue: initialSelectedFileId)
    }

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
    /// The read of the selected file, moved as the selection changes.
    @State
    private var fileReader: DetailReader<BridgeFile>?
    /// The selected file as last read; `nil` while none is selected, until
    /// its read answers, and while the library holds no such file.
    @State
    private var selectedFile: BridgeFile?

    var body: some View {
        Group {
            if let error {
                Notice(
                    error: error,
                    look: .centered,
                    action: error.retryAction {
                        reader?.retry()
                        fileReader?.retry()
                    }
                )
            }
            else if let inspector {
                VStack(spacing: 0) {
                    StorageTransferControls(items: inspector.transfers)
                    fileList(inspector.files)
                    if let selectedFile {
                        Divider()
                        StorageFileDetails(file: selectedFile) {
                            selectedFileId = nil
                        }
                    }
                }
            }
            else {
                // The release's read is local; nothing shows until it lands.
                Color.clear
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear {
            showRelease(releaseId)
            showFile(selectedFileId)
        }
        .onChange(of: releaseId) { _, newId in
            selectedFileId = nil
            showRelease(newId)
        }
        .onChange(of: selectedFileId) { _, newId in showFile(newId) }
        .onDisappear {
            reader?.close()
            fileReader?.close()
        }
    }

    private func fileList(_ rows: [BridgeStorageInspectorFile]) -> some View {
        Group {
            if rows.isEmpty {
                ContentUnavailableView { Text("No files") }
            }
            else {
                AccentSelectionList(
                    rows: rows,
                    id: \.file.id,
                    selection: $selectedFileId
                ) { row in
                    StorageInspectorFileRow(row: row)
                }
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

    /// Read the file `fileId` names for its details, or end the read when no
    /// file is selected.
    private func showFile(_ fileId: String?) {
        guard let fileId else {
            fileReader?.close()
            selectedFile = nil
            return
        }
        // Another file's value shows nothing of this one.
        if fileId != fileReader?.id {
            selectedFile = nil
        }
        let fileReader =
            fileReader
            ?? DetailReader(
                open: library.storageFile,
                onValue: { _, value in selectedFile = value },
                onError: { _, error in self.error = DisplayError(error) }
            )
        self.fileReader = fileReader
        fileReader.show(fileId)
    }
}
