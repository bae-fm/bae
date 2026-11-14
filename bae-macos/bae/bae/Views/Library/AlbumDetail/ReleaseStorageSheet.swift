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
    /// The album's title, which Unpin's alert names the release by.
    let albumTitle: String
    /// Whether Unpin asks first (the person's "Ask before unpinning").
    let asksBeforeUnpinning: Bool
    let onAction: (BridgeReleaseStorageAction) -> Void
    /// Unpin, after the alert asked: `dontShowAgain` is its box.
    let onUnpinConfirmed: (_ dontShowAgain: Bool) -> Void
    let onCancelPin: () -> Void
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
    /// The release Unpin's alert is asking about, while it shows.
    @State
    private var pendingUnpin: PendingUnpin?

    /// The sheet for `release`, which its read then follows.
    init(
        release: ReleaseDetail,
        albumTitle: String,
        asksBeforeUnpinning: Bool,
        onAction: @escaping (BridgeReleaseStorageAction) -> Void,
        onUnpinConfirmed: @escaping (Bool) -> Void,
        onCancelPin: @escaping () -> Void,
        onExport: @escaping () -> Void,
        onSaveAs: @escaping () -> Void,
        onDone: @escaping () -> Void
    ) {
        releaseId = release.id
        self.albumTitle = albumTitle
        self.asksBeforeUnpinning = asksBeforeUnpinning
        self.onAction = onAction
        self.onUnpinConfirmed = onUnpinConfirmed
        self.onCancelPin = onCancelPin
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
            .padding(.horizontal, ThemeSpace.edge)
            .padding(.vertical, 14)
            Divider()
            if let releaseError {
                Notice(error: releaseError)
                    .padding()
            }
            else {
                StorageStatusBand(
                    release: release,
                    onAction: act,
                    onCancelPin: onCancelPin,
                    onExport: onExport,
                    onSaveAs: onSaveAs
                )
            }
            Divider()
            filesSection
        }
        .onAppear { showRelease() }
        .onDisappear { releaseReader?.close() }
        .unpinConfirmation(pending: $pendingUnpin) { _, answer in
            if case .unpin(let dontShowAgain) = answer {
                onUnpinConfirmed(dontShowAgain)
            }
        }
    }

    /// A storage action from the band: Unpin asks first while the setting
    /// says to, the rest go straight on.
    private func act(_ action: BridgeReleaseStorageAction) {
        if action == .unpin,
            UnpinAsk.asks(askBeforeUnpinning: asksBeforeUnpinning)
        {
            pendingUnpin = PendingUnpin(
                releaseIds: [releaseId],
                title: albumTitle
            )
        }
        else {
            onAction(action)
        }
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
                Notice(
                    error: error,
                    look: .centered,
                    action: error.retryAction {
                        self.error = nil
                        reader?.retry()
                    }
                )
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
                Text(file.originalFilename).cutLine(.fileName)
            }
            TableColumn(localizedCoreString("core.audio.label")) { file in
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
