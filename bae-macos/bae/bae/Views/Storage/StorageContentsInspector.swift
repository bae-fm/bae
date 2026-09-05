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

    @State
    private var observationTask: Task<Void, Never>?

    var body: some View {
        VStack(spacing: 0) {
            StorageTransferControls(releaseId: releaseId)
            fileList
        }
        .onAppear { startObservation() }
        .onChange(of: releaseId) { _, _ in startObservation() }
        .onDisappear { observationTask?.cancel() }
    }

    private var fileList: some View {
        Group {
            if let error = libraryStore.releaseDetailErrors[releaseId] {
                LoadFailureView(line: error.line) {
                    startObservation()
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

    private func startObservation() {
        observationTask?.cancel()
        observationTask = Task { @MainActor in
            await libraryStore.observeReleaseDetail(
                releaseId: releaseId,
                library: library,
                onValue: {}
            )
        }
    }
}
