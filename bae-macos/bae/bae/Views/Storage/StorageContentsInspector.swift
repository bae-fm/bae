import BaeKit
import SwiftUI

/// The files belonging to the selected release. Detail observation lives here
/// so list rows depend only on their summary projection.
struct StorageContentsInspector: View {
    @Environment(Library.self)
    private var library
    @Environment(LibraryStore.self)
    private var libraryStore

    let releaseId: String

    @State
    private var observationTask: Task<Void, Never>?

    var body: some View {
        Group {
            if let error = libraryStore.releaseDetailErrors[releaseId] {
                LoadFailureView(line: error.line) {
                    startObservation()
                }
            }
            else if let detail = libraryStore.releaseDetails[releaseId] {
                if detail.files.isEmpty {
                    ContentUnavailableView(
                        "No files",
                        systemImage: "doc"
                    )
                }
                else {
                    Table(detail.files) {
                        TableColumn("Name") { file in
                            Text(file.originalFilename)
                                .lineLimit(1)
                        }
                        TableColumn(coreString("core.audio.label")) { file in
                            if let format = file.audioFormat {
                                Text(format.text)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .width(min: 60, ideal: 80, max: 110)
                        TableColumn("Size") { file in
                            Text(file.fileSizeText)
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        }
                        .width(min: 60, ideal: 80, max: 100)
                    }
                }
            }
            else {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear { startObservation() }
        .onChange(of: releaseId) { _, _ in startObservation() }
        .onDisappear { observationTask?.cancel() }
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
