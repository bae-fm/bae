import BaeKit
import SwiftUI

/// The "Identifying N / total" line and thin bar, fed by the queue sweep's
/// progress event. Reached through `QueueProgressIndicator`'s popover: a
/// sweep that finishes on its own does not earn a permanent row above every
/// tab. The caller shows this only while there is something left to say —
/// see `ImportCandidateListContent` — rather than this view deciding that
/// for itself.
///
/// The line is a control, not a label. The candidates the count is waiting on
/// are rows somewhere in the queue, and a number that sits still while giving
/// no way to reach what it is waiting on is the frustrating half of this pane.
/// Tapping it goes to the first one.
struct QueueProgressView: View {
    let identified: UInt32
    let total: UInt32
    /// Go to the first candidate with no verdict yet. Nil when there is none
    /// to go to, which is also when the count has nothing left to wait on.
    let onGoToUnidentified: (() -> Void)?

    private var fraction: Double {
        total == 0 ? 1 : Double(identified) / Double(total)
    }

    var body: some View {
        Button {
            onGoToUnidentified?()
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack {
                    Text("Identifying")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                    Spacer()
                    Text(
                        verbatim:
                            "\(identified.formatted()) / \(total.formatted())"
                    )
                    .font(.system(size: 12))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                }
                ProgressTrackBar(progress: fraction, trackHeight: 3)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(onGoToUnidentified == nil)
        .help("Go to a candidate still being identified")
    }
}

/// The filter row's compact stand-in for the line above: a ring at the sweep's
/// fraction and how many candidates it still has to reach, opening the line
/// itself on click. The ring is a glance — the numbers are in the popover.
struct QueueProgressIndicator: View {
    let identified: UInt32
    let total: UInt32
    let onGoToUnidentified: (() -> Void)?

    @State
    private var lineShown = false

    private var fraction: Double {
        total == 0 ? 0 : Double(identified) / Double(total)
    }

    var body: some View {
        Button {
            lineShown = true
        } label: {
            HStack(spacing: 4) {
                ring
                // The same two numbers the popover shows, so the glance and
                // the line it opens never disagree. Verbatim: this is digits
                // and a separator, and each number is already formatted for
                // the locale.
                Text(
                    verbatim:
                        "\(identified.formatted()) / \(total.formatted())"
                )
                .font(.system(size: 11.5))
                .monospacedDigit()
            }
            .foregroundStyle(.secondary)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Identifying")
        .popover(isPresented: $lineShown, arrowEdge: .bottom) {
            QueueProgressView(
                identified: identified,
                total: total,
                onGoToUnidentified: onGoToUnidentified
            )
            .frame(width: 220)
            .padding(12)
            .background { PopoverBehavior() }
        }
    }

    private var ring: some View {
        ZStack {
            Circle()
                .stroke(Color.secondary.opacity(0.25), lineWidth: 2)
            Circle()
                .trim(from: 0, to: fraction)
                .stroke(
                    Theme.accent,
                    style: StrokeStyle(lineWidth: 2, lineCap: .round)
                )
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 11, height: 11)
    }
}

/// The filter row's one signal that watched folders are being scanned. Core
/// supplies both the total and the per-root current-generation counts; this
/// view only formats and renders them.
struct FolderScanProgressIndicator: View {
    let activity: BridgeFolderScanActivity

    @State
    private var detailsShown = false

    var body: some View {
        Button {
            detailsShown = true
        } label: {
            HStack(spacing: 4) {
                ProgressView()
                    .controlSize(.small)
                    .frame(width: 11, height: 11)
                Text(
                    verbatim: coreString(
                        "ui.import.scan.found",
                        Int(activity.foundCount)
                    )
                )
                .font(.system(size: 11.5))
                .monospacedDigit()
            }
            .foregroundStyle(.secondary)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(coreString("ui.import.scan.activity"))
        .popover(isPresented: $detailsShown, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(activity.folders, id: \.watchedFolderPath) { folder in
                    HStack(spacing: 12) {
                        Text(verbatim: folder.watchedFolderName)
                            .lineLimit(1)
                        Spacer(minLength: 12)
                        Text(
                            verbatim: coreString(
                                "ui.import.scan.found",
                                Int(folder.foundCount)
                            )
                        )
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                    }
                }
            }
            .font(.system(size: 12))
            .frame(width: 240)
            .padding(12)
            .background { PopoverBehavior() }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Queue progress indicator") {
        QueueProgressIndicator(
            identified: 112,
            total: 130,
            onGoToUnidentified: {}
        )
        .padding()
        .windowBackground()
    }

    #Preview("Queue progress") {
        QueueProgressView(
            identified: 112,
            total: 130,
            onGoToUnidentified: {}
        )
        .padding()
        .frame(width: 280)
        .windowBackground()
    }

    #Preview("Folder scan progress") {
        FolderScanProgressIndicator(
            activity: BridgeFolderScanActivity(
                foundCount: 179,
                folders: [
                    BridgeActiveFolderScan(
                        watchedFolderPath: "/imports/one",
                        watchedFolderName: "Incoming",
                        foundCount: 124
                    ),
                    BridgeActiveFolderScan(
                        watchedFolderPath: "/imports/two",
                        watchedFolderName: "Archive",
                        foundCount: 55
                    ),
                ]
            )
        )
        .padding()
        .windowBackground()
    }
#endif
