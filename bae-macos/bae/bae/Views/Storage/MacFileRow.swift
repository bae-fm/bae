import BaeKit
import SwiftUI

/// One file in the storage inspector, on one line: a mark for where its
/// bytes are going, its name and its size. A file at rest, stored or kept,
/// has no mark; one queued to upload, or waiting to be kept while its
/// release pins, a dashed circle, its name dimmed; one being sent or
/// downloaded to be kept a ring that fills as it goes, its name in the
/// transfer's colour. A file whose original is missing, so that its upload
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
        case queued(StorageTransferKind)
        case sending(fileId: String)
        case pinning(fileId: String)
    }

    private var place: Place {
        if let problem = row.problem {
            return .stopped(problem)
        }
        if let upload = row.upload {
            switch upload.state {
            case .waiting: return .queued(.upload)
            case .uploading: return .sending(fileId: upload.fileId)
            }
        }
        switch row.pin {
        case .waiting: return .queued(.pin)
        case .downloading(let fileId): return .pinning(fileId: fileId)
        case .kept, nil: return .stored
        }
    }

    @ViewBuilder
    private var mark: some View {
        switch place {
        case .stored: StorageFileMark(state: .stored)
        case .stopped(let problem): StorageFileMark(state: .stopped(problem))
        case .queued(let kind): StorageFileMark(state: .queued(kind))
        case .sending(let fileId): StorageSendingFileMark(fileId: fileId)
        case .pinning(let fileId): StoragePinningFileMark(fileId: fileId)
        }
    }

    private var nameStyle: AnyShapeStyle {
        switch place {
        case .stored, .stopped: AnyShapeStyle(.primary)
        case .queued: AnyShapeStyle(Theme.macTextSecondary)
        case .sending, .pinning: AnyShapeStyle(Theme.accent)
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
            state: .moving(.upload, outboxStore.fileShare(forFile: fileId))
        )
    }
}

/// A file being downloaded to be kept's ring, filled each frame from the
/// file's latest share; SwiftUI hands it only the stream.
private struct StoragePinningFileMark: View {
    let fileId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StorageFileMark(
            state: .moving(.pin, downloadStore.fileShare(forFile: fileId))
        )
    }
}

/// A file row's mark: nothing for a file at rest, a dashed circle for one
/// waiting its turn, a tone's glyph for a file whose original is missing or
/// changed, a ring filling as far as its transfer has come, which
/// the ring's own view sets from the file's stream.
struct StorageFileMark: View {
    enum State {
        case stored
        case stopped(BridgeFileProblem)
        case queued(StorageTransferKind)
        case moving(StorageTransferKind, LatestSource<Double>)
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
        case .queued(let kind):
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
                .accessibilityLabel(queuedLabel(kind))
        case .moving(let kind, let source):
            MacProgressRing(source: source)
                .accessibilityElement()
                .accessibilityLabel(movingLabel(kind))
        }
    }

    /// What the file waits to do: upload, or be pinned.
    private func queuedLabel(_ kind: StorageTransferKind) -> Text {
        switch kind {
        case .upload: Text(verbatim: BridgeUploadActivity.queued.phaseText)
        case .pin: Text("Queued to pin")
        }
    }

    private func movingLabel(_ kind: StorageTransferKind) -> Text {
        switch kind {
        case .upload: Text("Uploading")
        case .pin: Text("Pinning")
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
