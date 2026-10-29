import BaeKit
import SwiftUI

/// A release's Storage sheet: its storage status band over a sortable table of
/// its files, which core reads in the order the table sorts by.
struct ReleaseStorageSheet: View {
    @Environment(Library.self)
    private var library

    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onDone: () -> Void

    /// The file table's sort, which names the column and direction core
    /// reads the files in; the Audio column has none, as its value is
    /// optional.
    @State
    private var sortOrder: [KeyPathComparator<BridgeFile>] = [
        KeyPathComparator(
            \BridgeFile.originalFilename,
            comparator: .localizedStandard
        )
    ]
    /// The sheet's one read of the release's files, moved as the sort
    /// changes.
    @State
    private var reader: ReleaseFilesReader?
    /// The files as last read, in the sort's order; empty until they are.
    @State
    private var files: [BridgeFile] = []
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Storage")
                    .themeText(.heading)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()
            Divider()
            StorageStatusBand(
                release: release,
                onAction: onAction,
                onExport: onExport,
                onSaveAs: onSaveAs
            )
            Divider()
            filesSection
        }
    }

    private var filesSection: some View {
        VStack(spacing: 0) {
            HStack {
                Eyebrow("Files")
                Spacer()
            }
            .padding(.horizontal)
            .padding(.top, ThemeSpace.related)
            .padding(.bottom, ThemeSpace.inline)

            if let error {
                LoadFailureView(error: error) {
                    self.error = nil
                    reader?.retry()
                }
            }
            else {
                filesTable
            }
        }
        .onAppear { showFiles() }
        .onChange(of: sortOrder) { showFiles() }
        .onDisappear { reader?.close() }
    }

    private var filesTable: some View {
        // The rows come in the sort's order from core, so the table sorts
        // nothing itself: each column's comparator only names what it sorts by.
        Table(files, sortOrder: $sortOrder) {
            TableColumn(
                "Name",
                value: \.originalFilename,
                comparator: .localizedStandard
            ) { file in
                Text(file.originalFilename).lineLimit(1)
            }
            TableColumn(coreString("core.audio.label")) { file in
                // Non-audio files (images, cue) leave the cell empty.
                if let format = file.audioFormat {
                    Text(format.text)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            TableColumn("Size", value: \.fileSize) { file in
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .width(min: 70, ideal: 80)
            TableColumn("Kind", value: \.contentType) { file in
                Text(file.contentType)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            .width(min: 90, ideal: 130)
        }
    }

    /// The order core reads the files in for the table's sort: its first
    /// column, and that column's direction.
    private var fileOrder: BridgeReleaseFileOrder {
        let column = sortOrder.first
        let keyPath: AnyKeyPath? = column?.keyPath
        let size: AnyKeyPath = \BridgeFile.fileSize
        let kind: AnyKeyPath = \BridgeFile.contentType
        let by: BridgeReleaseFileColumn =
            if keyPath == size {
                .size
            }
            else if keyPath == kind {
                .kind
            }
            else {
                .name
            }
        return BridgeReleaseFileOrder(
            column: by,
            descending: column?.order == .reverse
        )
    }

    private func showFiles() {
        error = nil
        let reader =
            reader
            ?? ReleaseFilesReader(
                open: library.releaseFiles,
                onFiles: { read in
                    files = read ?? []
                    error = nil
                },
                onError: { error = DisplayError($0) }
            )
        self.reader = reader
        reader.show(
            BridgeReleaseFilesRequest(releaseId: release.id, order: fileOrder)
        )
    }
}

#if DEBUG
    #Preview("Release Storage Sheet") {
        ReleaseStorageSheet(
            release: PreviewData.storageRelease(
                storageState: .remote,
                pinned: false,
                storageActions: [.pin, .detachAudio]
            ),
            onAction: { _ in },
            onExport: {},
            onSaveAs: {},
            onDone: {},
        )
        .frame(width: 640, height: 600)
        .background(Theme.background)
        .environment(PreviewData.albumDetailLibrary())
        .environment(OutboxStore(uploadsPaused: false))
        .preferredColorScheme(.dark)
    }
#endif
