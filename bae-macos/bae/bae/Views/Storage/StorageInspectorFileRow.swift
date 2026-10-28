import BaeKit
import SwiftUI

/// One file in the storage inspector, on one line: a mark for where its
/// bytes are going, its name and its size. A file at rest, stored or kept,
/// has no mark; one queued to upload, or waiting to be kept while its
/// release pins, a dashed circle, its name dimmed; one being sent a ring
/// that fills as it goes, its name in the upload's colour. A file waiting on
/// a reason says why in its tooltip.
struct StorageInspectorFileRow: View {
    let row: BridgeStorageInspectorFile

    /// The mark's side, which every row keeps so the names line up, and the
    /// gap between the row's columns.
    private static let markSize: CGFloat = 16
    private static let columnGap: CGFloat = 10

    var body: some View {
        HStack(spacing: Self.columnGap) {
            StorageFileMark(state: state)
                .frame(width: Self.markSize, height: Self.markSize)
            Text(row.file.originalFilename)
                .lineLimit(1)
                .truncationMode(.middle)
                .foregroundStyle(nameStyle)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(row.file.fileSizeText)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .padding(.vertical, ThemeSpace.related)
        // The row's own padding is its whole height: the list adds none.
        .listRowInsets(
            EdgeInsets(
                top: 0,
                leading: ThemeSpace.edge,
                bottom: 0,
                trailing: ThemeSpace.edge
            )
        )
        .help(help)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("file:\(row.file.id)")
    }

    private var state: StorageFileMark.State {
        if let upload = row.upload {
            switch upload.state {
            case .waiting: return .queued
            case .uploading:
                return .moving(.upload, fraction: upload.bar?.fraction)
            }
        }
        switch row.pin {
        case .waiting: return .queued
        case .kept, nil: return .stored
        }
    }

    private var nameStyle: AnyShapeStyle {
        switch state {
        case .stored: AnyShapeStyle(.primary)
        case .queued: AnyShapeStyle(.secondary)
        case .moving(let kind, _): AnyShapeStyle(kind.color)
        }
    }

    /// Why a waiting upload waits, as coven says; otherwise the file's name,
    /// which the row may cut short.
    private var help: String {
        if let pending = row.upload?.pending {
            return localizedCoreString(pending.reason.key)
        }
        return row.file.originalFilename
    }
}

/// A file row's mark: nothing for a file at rest, a dashed circle for one
/// waiting its turn, a ring filling as far as its transfer has come.
struct StorageFileMark: View {
    enum State: Equatable {
        case stored
        case queued
        /// `fraction` is `nil` while nothing has been confirmed sent.
        case moving(StorageTransferKind, fraction: Double?)
    }

    let state: State

    /// The ring's width and the dashed circle's.
    private static let lineWidth: CGFloat = 1.75

    var body: some View {
        // Every mark stays in the row's layout; only the one that applies
        // shows.
        ZStack {
            Circle()
                .inset(by: Self.lineWidth)
                .stroke(
                    .secondary,
                    style: StrokeStyle(lineWidth: Self.lineWidth, dash: [2, 2])
                )
                .opacity(state == .queued ? 1 : 0)
            Circle()
                .inset(by: Self.lineWidth)
                .stroke(Theme.hairline, lineWidth: Self.lineWidth)
                .opacity(ringShown ? 1 : 0)
            Circle()
                .inset(by: Self.lineWidth)
                .trim(from: 0, to: ringFraction)
                .stroke(
                    ringColor,
                    style: StrokeStyle(
                        lineWidth: Self.lineWidth,
                        lineCap: .round
                    )
                )
                .rotationEffect(.degrees(-90))
                .opacity(ringShown ? 1 : 0)
        }
        .accessibilityElement()
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHidden(state == .stored)
    }

    private var ringShown: Bool {
        if case .moving = state { return true }
        return false
    }

    /// How much of the ring fills: as far as the transfer has come, none
    /// before anything is confirmed and for a mark that shows no ring.
    private var ringFraction: Double {
        switch state {
        case .moving(_, let fraction?): fraction
        case .moving(_, nil), .stored, .queued: 0
        }
    }

    private var ringColor: Color {
        switch state {
        case .moving(let kind, _): kind.color
        case .stored, .queued: .clear
        }
    }

    private var accessibilityLabel: Text {
        switch state {
        case .stored: Text(verbatim: "")
        case .queued: Text("Queued")
        case .moving(.upload, _): Text("Uploading")
        case .moving(.pin, _): Text("Pinning")
        }
    }
}

#if DEBUG
    #Preview("File marks") {
        HStack(spacing: ThemeSpace.group) {
            StorageFileMark(state: .stored)
            StorageFileMark(state: .queued)
            StorageFileMark(state: .moving(.upload, fraction: 0.38))
            StorageFileMark(state: .moving(.pin, fraction: 0.64))
            StorageFileMark(state: .moving(.upload, fraction: nil))
        }
        .frame(height: 16)
        .padding()
    }
#endif
