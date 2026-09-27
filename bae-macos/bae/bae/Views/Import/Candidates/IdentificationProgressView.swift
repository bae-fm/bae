import BaeKit
import SwiftUI

/// The "Identifying [bar] N / total" line in
/// `IdentificationProgressIndicator`'s popover; tapping it goes to the first
/// candidate still being identified.
struct IdentificationProgressView: View {
    let identified: UInt32
    let total: UInt32
    /// Go to the first candidate the count is still waiting on.
    let onGoToUnidentified: () -> Void
    /// Take every candidate off the identification queue.
    let onCancelAll: () -> Void

    private var fraction: Double {
        total == 0 ? 1 : Double(identified) / Double(total)
    }

    var body: some View {
        VStack(alignment: .trailing, spacing: 8) {
            Button {
                onGoToUnidentified()
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
            .help("Go to a candidate still being identified")
            Button("Cancel All", role: .destructive, action: onCancelAll)
                .controlSize(.small)
                .help(
                    "Stop identifying every candidate still waiting or running"
                )
        }
    }
}

/// The filter row's identification progress ring; clicking it opens
/// `IdentificationProgressView`.
struct IdentificationProgressIndicator: View {
    let identified: UInt32
    let total: UInt32
    let onGoToUnidentified: () -> Void
    let onCancelAll: () -> Void

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
                onGoToUnidentified: onGoToUnidentified,
                onCancelAll: {
                    lineShown = false
                    onCancelAll()
                }
            )
            .frame(width: 220)
            .padding(12)
            .background { PopoverBehavior() }
        }
    }

    private var ring: some View {
        ZStack {
            Circle()
                .stroke(Theme.hairlineStrong, lineWidth: 2)
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

/// The filter row's watched-folder scan indicator; clicking it lists each
/// root's found count.
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
            onGoToUnidentified: {},
            onCancelAll: {}
        )
        .padding()
        .windowBackground()
    }

    #Preview("Identification progress") {
        IdentificationProgressView(
            identified: 112,
            total: 130,
            onGoToUnidentified: {},
            onCancelAll: {}
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
