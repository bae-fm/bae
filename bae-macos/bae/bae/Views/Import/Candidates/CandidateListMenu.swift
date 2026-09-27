import BaeKit
import SwiftUI

/// The candidate list's menu: sort order, Pending filter, watched folders,
/// and expanding or collapsing every folder group.
///
/// `Equatable` over what the menu draws, and rendered through `.equatable()`,
/// because a `Menu` rebuilt while open closes under the pointer.
struct CandidateListMenu: View, Equatable {
    let watchedFolders: [BridgeWatchedFolder]
    /// Roots with a refresh in flight; their Refresh entry is disabled.
    let refreshingFolders: Set<String>
    /// Each root's scan state, by path. A failed scan also marks the trigger.
    let scanStatuses: [String: BridgeFolderScanStatus]
    /// Roots on a network volume, which are checked on a schedule rather than
    /// reported the moment they change.
    let networkFolders: Set<String>
    /// Whether the queue has folder groups; without any, Expand All and
    /// Collapse All are disabled.
    let hasGroups: Bool
    let sortOrder: BridgeImportListOrder
    let onSetSortOrder: (BridgeImportListOrder) -> Void
    /// Which of Pending's rows the list shows; `nil` shows them all.
    let pendingFilter: BridgePendingFilter?
    /// Whether the tab on show is Pending, the one tab the filter applies to.
    let pendingFilterApplies: Bool
    let onSetPendingFilter: (BridgePendingFilter?) -> Void
    let onAddFolder: () -> Void
    /// Fold every folder group in the queue open (`true`) or shut (`false`).
    let onSetAllGroupsExpanded: (_ expanded: Bool) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Stop watching `path`.
    let onRemoveFolder: (_ path: String) -> Void

    /// Leaves out the closures, which are new on every render.
    nonisolated static func == (
        lhs: CandidateListMenu,
        rhs: CandidateListMenu
    ) -> Bool {
        lhs.watchedFolders == rhs.watchedFolders
            && lhs.refreshingFolders == rhs.refreshingFolders
            && lhs.networkFolders == rhs.networkFolders
            && lhs.hasGroups == rhs.hasGroups
            && lhs.sortOrder == rhs.sortOrder
            && lhs.pendingFilter == rhs.pendingFilter
            && lhs.pendingFilterApplies == rhs.pendingFilterApplies
            && hasFailedScan(in: lhs.scanStatuses)
                == hasFailedScan(in: rhs.scanStatuses)
            && lhs.watchedFolders.allSatisfy { folder in
                scanPresentation(
                    lhs.scanStatuses[folder.path],
                    equals: rhs.scanStatuses[folder.path]
                )
            }
    }

    private var hasFailedScan: Bool {
        Self.hasFailedScan(in: scanStatuses)
    }

    nonisolated private static func hasFailedScan(
        in statuses: [String: BridgeFolderScanStatus]
    ) -> Bool {
        statuses.values.contains { status in
            if case .failed = status { return true }
            return false
        }
    }

    /// Whether two scan states draw the same entry; a scan's found count is
    /// ignored because the entry shows one spinner for the whole scan.
    nonisolated private static func scanPresentation(
        _ lhs: BridgeFolderScanStatus?,
        equals rhs: BridgeFolderScanStatus?
    ) -> Bool {
        switch (lhs, rhs) {
        case (.scanning, .scanning):
            true
        case (.failed(let lhsError), .failed(let rhsError)):
            lhsError == rhsError
        case (.complete, .complete), (.complete, nil), (nil, .complete),
            (nil, nil):
            true
        default:
            false
        }
    }

    var body: some View {
        Menu {
            Picker(
                "Sort",
                selection: Binding(get: { sortOrder }, set: onSetSortOrder)
            ) {
                Text("Newest First").tag(BridgeImportListOrder.newestFirst)
                Text("Oldest First").tag(BridgeImportListOrder.oldestFirst)
                Text("Folder Name (A–Z)")
                    .tag(BridgeImportListOrder.pathAscending)
                Text("Folder Name (Z–A)")
                    .tag(BridgeImportListOrder.pathDescending)
            }
            .pickerStyle(.inline)
            PendingFilterPicker(
                selection: pendingFilter,
                onSelect: onSetPendingFilter
            )
            .disabled(!pendingFilterApplies)
            Section("Folders") {
                Button {
                    onAddFolder()
                } label: {
                    Label("Add a Folder\u{2026}", systemImage: "plus")
                }
                ForEach(watchedFolders, id: \.path) { folder in
                    folderMenu(folder)
                }
            }
            Section("Groups") {
                Button {
                    onSetAllGroupsExpanded(true)
                } label: {
                    Label("Expand All", systemImage: "chevron.down")
                }
                .disabled(!hasGroups)
                Button {
                    onSetAllGroupsExpanded(false)
                } label: {
                    Label("Collapse All", systemImage: "chevron.right")
                }
                .disabled(!hasGroups)
            }
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(.system(size: ImportFilterBarLayout.glyphSize))
                .overlay(alignment: .topTrailing) {
                    // A filter hiding Pending rows marks the trigger, so a
                    // short list never reads as a short queue.
                    if pendingFilter != nil && !hasFailedScan {
                        Circle()
                            .fill(Color.accentColor)
                            .frame(width: 6, height: 6)
                            .offset(x: 2, y: -2)
                    }
                    if hasFailedScan {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.system(size: 8))
                            .foregroundStyle(Theme.danger)
                            .offset(x: 3, y: -3)
                    }
                }
                .filterBarControl()
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Sorting, Filtering and Watched Folders")
    }

    /// One root's entry: its scan state is the icon, and a failed scan's error
    /// is the tooltip.
    @ViewBuilder
    private func folderMenu(_ folder: BridgeWatchedFolder) -> some View {
        let onNetwork = networkFolders.contains(folder.path)
        switch scanStatuses[folder.path] {
        case .scanning:
            folderEntry(folder) {
                ProgressView().controlSize(.small)
            }
            .help(networkLine(onNetwork) ?? "")
        case .failed(let error):
            folderEntry(folder) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(Theme.danger)
            }
            .help(
                [error, networkLine(onNetwork)]
                    .compactMap { $0 }
                    .joined(separator: "\n")
            )
        case .complete, nil:
            folderEntry(folder) {
                if onNetwork {
                    Image(systemName: "network")
                        .foregroundStyle(.secondary)
                }
                else {
                    EmptyView()
                }
            }
            .help(networkLine(onNetwork) ?? "")
        }
    }

    /// The tooltip line saying how a network folder is checked; `nil` for a
    /// local folder.
    private func networkLine(_ onNetwork: Bool) -> String? {
        guard onNetwork else { return nil }
        return coreString(
            bridgeNetworkFolderWatchKey(),
            Int(bridgeNetworkFolderCheckMinutes())
        )
    }

    private func folderEntry<Icon: View>(
        _ folder: BridgeWatchedFolder,
        @ViewBuilder icon: () -> Icon
    ) -> some View {
        Menu {
            let refreshing = refreshingFolders.contains(folder.path)
            Button {
                onRefreshFolder(folder)
            } label: {
                Label(
                    refreshing ? "Refreshing\u{2026}" : "Refresh",
                    systemImage: "arrow.clockwise"
                )
            }
            .disabled(refreshing)
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: folder.path)
            }
            Divider()
            Button("Remove Folder", role: .destructive) {
                onRemoveFolder(folder.path)
            }
        } label: {
            Label {
                Text(folder.name)
            } icon: {
                icon()
            }
        }
    }
}

#if DEBUG
    #Preview("Candidate list menu") {
        CandidateListMenu(
            watchedFolders: [
                PreviewData.importWatchedFolder,
                BridgeWatchedFolder(path: "/Music/Rips", name: "Rips"),
                BridgeWatchedFolder(path: "/Volumes/Vault", name: "Vault"),
            ],
            refreshingFolders: [],
            scanStatuses: [
                "/Music/Rips": .scanning(foundCount: 8),
                "/Volumes/Vault": .failed(
                    error: "The volume could not be reached."
                ),
            ],
            networkFolders: ["/Volumes/Vault"],
            hasGroups: true,
            sortOrder: .newestFirst,
            onSetSortOrder: { _ in },
            pendingFilter: .identified,
            pendingFilterApplies: true,
            onSetPendingFilter: { _ in },
            onAddFolder: {},
            onSetAllGroupsExpanded: { _ in },
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
        .padding()
        .windowBackground()
    }
#endif
