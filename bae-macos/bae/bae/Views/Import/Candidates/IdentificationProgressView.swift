import BaeKit
import SwiftUI

/// The "Identifying [bar] N / total" line, fed by core's identification
/// progress event. Reached through `IdentificationProgressIndicator`'s
/// popover: identifications that finish on their own do not earn a permanent
/// row above every tab. The caller shows this only while there is something
/// left to say — see `ImportCandidateListContent` — rather than this view
/// deciding that for itself.
///
/// The line is a control, not a label. The candidates the count is waiting on
/// are rows somewhere in the queue, and a number that sits still while giving
/// no way to reach what it is waiting on is the frustrating half of this pane.
/// Tapping it goes to the first one.
struct IdentificationProgressView: View {
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
            ProgressLine(
                String(localized: "Identifying"),
                progress: fraction,
                detail: "\(identified.formatted()) / \(total.formatted())"
            )
            .font(.system(size: 12))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(onGoToUnidentified == nil)
        .help("Go to a candidate still being identified")
    }
}

/// The filter row's compact stand-in for the line above: a ring at the
/// identifications' fraction, opening the counts on click.
struct IdentificationProgressIndicator: View {
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
            ring
                .foregroundStyle(.secondary)
                .filterBarControl()
        }
        .buttonStyle(.plain)
        .help("Identifying")
        .popover(isPresented: $lineShown, arrowEdge: .bottom) {
            IdentificationProgressView(
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
        .frame(
            width: ImportFilterBarLayout.glyphSize,
            height: ImportFilterBarLayout.glyphSize
        )
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
            ProgressView()
                .controlSize(.small)
                .frame(
                    width: ImportFilterBarLayout.glyphSize,
                    height: ImportFilterBarLayout.glyphSize
                )
                .foregroundStyle(.secondary)
                .filterBarControl()
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

    #Preview("Identification progress indicator") {
        IdentificationProgressIndicator(
            identified: 112,
            total: 130,
            onGoToUnidentified: {}
        )
        .padding()
        .windowBackground()
    }

    #Preview("Identification progress") {
        IdentificationProgressView(
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
