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
        VStack(alignment: .trailing, spacing: ThemeSpace.related) {
            Button {
                onGoToUnidentified()
            } label: {
                ProgressLine(
                    String(localized: "Identifying"),
                    progress: fraction,
                    detail: "\(identified.formatted()) / \(total.formatted())"
                )
                .themeText(.body)
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
            .padding(ThemeSpace.group)
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
            width: ImportFilterBarLayout.glyph.size,
            height: ImportFilterBarLayout.glyph.size
        )
    }
}

/// The filter row's place for the identification indicator, shown while a
/// batch is identifying. It reads the batch's progress itself, so each
/// identification that ends redraws this alone, not the list beside it.
struct IdentificationProgressSlot: View {
    let importStore: ImportStore
    let onGoToUnidentified: () -> Void
    let onCancelAll: () -> Void

    var body: some View {
        if let progress = importStore.identificationProgress,
            progress.total > 0
        {
            IdentificationProgressIndicator(
                identified: progress.identified,
                total: progress.total,
                onGoToUnidentified: onGoToUnidentified,
                onCancelAll: onCancelAll
            )
        }
    }
}

/// The filter row's place for the import indicator, shown while imports
/// run. It reads their count itself, so an import that starts or ends
/// redraws this alone.
struct ImportActivitySlot: View {
    let importStore: ImportStore
    let onCancelAll: () -> Void

    var body: some View {
        if importStore.importsInFlight > 0 {
            ImportActivityIndicator(
                count: importStore.importsInFlight,
                onCancelAll: onCancelAll
            )
        }
    }
}

/// The filter row's place for the scan indicator, shown while a root is
/// scanning. It reads the folder scans itself, so a scan's step redraws this
/// alone and nothing around it.
struct FolderScanActivitySlot: View {
    let importStore: ImportStore

    var body: some View {
        if let activity = importStore.folderScans.activity {
            FolderScanProgressIndicator(activity: activity)
        }
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
                    width: ImportFilterBarLayout.glyph.size,
                    height: ImportFilterBarLayout.glyph.size
                )
                .foregroundStyle(.secondary)
                .filterBarControl()
        }
        .buttonStyle(.plain)
        .help(coreString("ui.import.scan.activity"))
        .popover(isPresented: $detailsShown, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                ForEach(activity.folders, id: \.watchedFolderPath) { folder in
                    HStack(spacing: ThemeSpace.group) {
                        Text(verbatim: folder.watchedFolderName)
                            .lineLimit(1)
                        Spacer(minLength: ThemeSpace.group)
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
            .themeText(.body)
            .frame(width: 240)
            .padding(ThemeSpace.group)
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
