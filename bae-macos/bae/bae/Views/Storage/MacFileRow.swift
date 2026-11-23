import BaeKit
import SwiftUI

/// One file in the storage inspector, on one line: a mark for where its
/// bytes are going, its name and its size. A file at rest, stored or kept,
/// has no mark; one waiting to upload, pin or move to This Mac has a
/// dashed circle and a dimmed name. A file uploading or downloading has
/// a ring that fills as it goes and a name in the transfer's colour. A file whose original is missing, so that its upload
/// waits for it, has a warning mark; one whose original changed, so that its
/// move failed, a danger mark. A file waiting on a reason says why in its
/// tooltip.
struct MacFileRow: View {
    let row: BridgeStorageInspectorFile

    var body: some View {
        MacItem {
            mark
                .frame(width: ThemeSize.lead, height: ThemeSize.lead)
        } content: {
            Text(row.file.originalFilename)
                .cutLine(.fileName)
                .foregroundStyle(nameStyle)
        } trail: {
            Text(row.file.fileSizeText)
                .monospacedDigit()
        }
        .padding(.vertical, ThemeSpace.related)
        // The row's own padding is its whole height. Sideways the row puts
        // its names on the edge the transfer rows above keep.
        .padding(.horizontal, ThemeSpace.edge)
        .help(help)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("file:\(row.file.id)")
    }

    /// Where the file's bytes are going, without how far: a moving file's
    /// mark reads that alone, by its file.
    private enum Place {
        case stored
        case stopped(BridgeFileProblem)
        case queuedUpload
        case queuedDownload(BridgeDownloadKind)
        case sending(fileId: String)
        case downloading(kind: BridgeDownloadKind, fileId: String)
    }

    private var place: Place {
        if let problem = row.problem {
            return .stopped(problem)
        }
        if let upload = row.upload {
            switch upload.state {
            case .waiting: return .queuedUpload
            case .uploading: return .sending(fileId: upload.fileId)
            }
        }
        switch row.download {
        case .waiting(let kind): return .queuedDownload(kind)
        case .downloading(let kind, let fileId):
            return .downloading(kind: kind, fileId: fileId)
        case .done, nil: return .stored
        }
    }

    @ViewBuilder
    private var mark: some View {
        switch place {
        case .stored: StorageFileMark(state: .stored)
        case .stopped(let problem): StorageFileMark(state: .stopped(problem))
        case .queuedUpload:
            StorageFileMark(
                state: .queued(
                    Text(verbatim: BridgeUploadActivity.queued.phaseText)
                )
            )
        case .queuedDownload(let kind):
            StorageFileMark(state: .queued(Text(verbatim: kind.queuedLabel)))
        case .sending(let fileId):
            StorageSendingFileMark(fileId: fileId)
        case .downloading(let kind, let fileId):
            StorageDownloadingFileMark(kind: kind, fileId: fileId)
        }
    }

    private var nameStyle: AnyShapeStyle {
        switch place {
        case .stored, .stopped: AnyShapeStyle(.primary)
        case .queuedUpload, .queuedDownload:
            AnyShapeStyle(Theme.macTextSecondary)
        case .sending, .downloading: AnyShapeStyle(Theme.accent)
        }
    }

    /// What a stopped file's mark says; why a waiting upload waits, as coven
    /// says; otherwise the file's name, which the row may cut short.
    private var help: String {
        if let problem = row.problem {
            return problem.words
        }
        if let pending = row.upload?.pending {
            return localizedCoreString(pending.reason.key)
        }
        return row.file.originalFilename
    }
}

/// A file being sent's ring, filled each frame from the file's latest share;
/// SwiftUI hands it only the stream.
private struct StorageSendingFileMark: View {
    let fileId: String
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        StorageFileMark(
            state: .moving(
                Text("Uploading"),
                outboxStore.fileShare(forFile: fileId)
            )
        )
    }
}

/// A downloading file's ring follows the file's stream for that operation.
private struct StorageDownloadingFileMark: View {
    let kind: BridgeDownloadKind
    let fileId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        let source =
            switch kind {
            case .pin: downloadStore.fileShare(forFile: fileId)
            case .moveToThisDevice:
                downloadStore.moveFileShares.source(for: fileId)
            }
        StorageFileMark(state: .moving(kind.movingFileLabel, source))
    }
}

/// A file row's mark: nothing for a file at rest, a dashed circle for one
/// waiting its turn, a tone's glyph for a file whose original is missing or
/// changed, a ring filling as far as its transfer has come, which
/// the ring's own view sets from the file's stream.
private struct StorageFileMark: View {
    enum State {
        case stored
        case stopped(BridgeFileProblem)
        case queued(Text)
        case moving(Text, LatestSource<Double>)
    }

    let state: State

    var body: some View {
        switch state {
        case .stored:
            Color.clear.accessibilityHidden(true)
        case .stopped(let problem):
            StatusGlyph(problem.tone)
                .accessibilityElement()
                .accessibilityLabel(Text(verbatim: problem.words))
        case .queued(let label):
            Circle()
                .inset(by: TransferRingNSView.lineWidth)
                .stroke(
                    Theme.macTextSecondary,
                    style: StrokeStyle(
                        lineWidth: TransferRingNSView.lineWidth,
                        dash: [2, 2]
                    )
                )
                .accessibilityElement()
                .accessibilityAddTraits(.isImage)
                .accessibilityLabel(label)
        case .moving(let label, let source):
            MacProgressRing(source: source)
                .accessibilityElement()
                .accessibilityAddTraits(.isImage)
                .accessibilityLabel(label)
        }
    }

}

extension BridgeDownloadKind {
    fileprivate var movingFileLabel: Text {
        switch self {
        case .pin: Text("Pinning")
        case .moveToThisDevice:
            Text(
                verbatim: BridgeReleaseStorageAction.moveToThisDevice
                    .transferLabel
            )
        }
    }
}

/// A stopped file's mark: its tone and what it says.
extension BridgeFileProblem {
    /// A missing original holds its upload, a warning; a changed one fails
    /// its move, a danger.
    fileprivate var tone: StatusTone {
        switch self {
        case .originalMissing: .warning
        case .originalChanged: .danger
        }
    }

    /// What the mark says: the words of the inspector's notice for the same
    /// file.
    fileprivate var words: String {
        switch self {
        case .originalMissing: String(localized: "Waiting for a file")
        case .originalChanged: String(localized: "Could not move to the cloud")
        }
    }
}
