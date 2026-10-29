import BaeKit
import SwiftUI

/// A release's Storage sheet: its storage status band over a sortable table of
/// its files, which core reads in the order the table sorts by. The release
/// follows its own live read, so a storage action's result shows while the
/// sheet is open.
struct ReleaseStorageSheet: View {
    @Environment(Library.self)
    private var library

    let releaseId: String
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onDone: () -> Void

    /// The release as its read last gave it, starting from the one the sheet
    /// opened on.
    @State
    private var release: ReleaseDetail
    /// Why the release's read gives nothing to show: it failed, or the
    /// release is no longer in the library.
    @State
    private var releaseError: DisplayError?
    /// The sheet's one read of the release.
    @State
    private var releaseReader: DetailReader<BridgeRelease>?

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

    /// The sheet for `release`, which its read then follows.
    init(
        release: ReleaseDetail,
        onAction: @escaping (BridgeReleaseStorageAction) -> Void,
        onExport: @escaping () -> Void,
        onSaveAs: @escaping () -> Void,
        onDone: @escaping () -> Void
    ) {
        releaseId = release.id
        self.onAction = onAction
        self.onExport = onExport
        self.onSaveAs = onSaveAs
        self.onDone = onDone
        _release = State(initialValue: release)
    }

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
            if let releaseError {
                ErrorDetailDisclosure(error: releaseError)
                    .padding()
            }
            else {
                StorageStatusBand(
                    release: release,
                    onAction: onAction,
                    onExport: onExport,
                    onSaveAs: onSaveAs
                )
            }
            Divider()
            filesSection
        }
        .onAppear { showRelease() }
        .onDisappear { releaseReader?.close() }
    }

    private func showRelease() {
        let reader =
            releaseReader
            ?? DetailReader(
                open: library.releaseDetail,
                onValue: { _, read in
                    guard let read else {
                        releaseError = DisplayError(
                            line: String(
                                localized:
                                    "This release is no longer in the library."
                            )
                        )
                        return
                    }
                    release = ReleaseDetail(bridge: read)
                    releaseError = nil
                },
                onError: { _, error in releaseError = DisplayError(error) }
            )
        releaseReader = reader
        reader.show(releaseId)
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
            BridgeReleaseFilesRequest(releaseId: releaseId, order: fileOrder)
        )
    }
}

#if DEBUG
    #Preview("Release Storage Sheet") {
        let release = PreviewData.storageBridgeRelease(
            storageState: .remote,
            pinned: false,
            storageActions: [.pin, .detachAudio]
        )
        ReleaseStorageSheet(
            release: ReleaseDetail(bridge: release),
            onAction: { _ in },
            onExport: {},
            onSaveAs: {},
            onDone: {},
        )
        .frame(width: 640, height: 600)
        .background(Theme.background)
        .environment(
            Library(
                releaseDetail: { .fixed { _ in release } },
                releaseFiles: { .fixed(PreviewData.previewReleaseFiles) }
            )
        )
        .environment(OutboxStore(uploadsPaused: false))
        .preferredColorScheme(.dark)
    }
#endif
