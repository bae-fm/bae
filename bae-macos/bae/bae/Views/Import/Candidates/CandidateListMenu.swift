import BaeKit
import SwiftUI

/// The watched folders the candidate list is built from: add a root, or
/// refresh, reveal, and remove one already being watched.
///
/// `Equatable` over the folders, their refresh state and their scans alone,
/// and rendered through `.equatable()`: the queue summary this is built from is
/// re-delivered on every verdict the sweep commits, and a `Menu` whose content
/// is rebuilt while it is open closes under the pointer. Comparing the values
/// the menu actually draws keeps it standing across those ticks.
struct CandidateListMenu: View, Equatable {
    let watchedFolders: [BridgeWatchedFolder]
    /// The roots with a refresh in flight — their entry says so and cannot be
    /// asked again.
    let refreshingFolders: Set<String>
    /// Where each root's scan stands, by path. A root being walked, or one
    /// whose walk failed, says so on its own entry — and a failure also marks
    /// the trigger, so nobody has to open the menu to find out.
    let scanStatuses: [String: BridgeFolderScanStatus]
    /// The roots on a volume served over the network. Their entry says so,
    /// because such a folder is checked on a schedule rather than reported the
    /// moment it changes — and an album added on the server that has not
    /// appeared yet is otherwise a mystery.
    let networkFolders: Set<String>
    let onAddFolder: () -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Stop watching `path`. Release grouping belongs to the queue below;
    /// removing a root stays an action here.
    let onRemoveFolder: (_ path: String) -> Void

    /// The actions are left out: each render hands over a fresh closure that
    /// does the same thing, so comparing them would say "changed" every time
    /// and defeat the point.
    nonisolated static func == (
        lhs: CandidateListMenu,
        rhs: CandidateListMenu
    ) -> Bool {
        lhs.watchedFolders == rhs.watchedFolders
            && lhs.refreshingFolders == rhs.refreshingFolders
            && lhs.networkFolders == rhs.networkFolders
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

    /// Compare exactly what a root's menu entry renders. Scan progress updates
    /// the queue's found count, but this menu renders the same spinner for the
    /// entire scan; replacing it for every count closes an open system menu.
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
        } label: {
            Image(systemName: "ellipsis.circle")
                .overlay(alignment: .topTrailing) {
                    if hasFailedScan {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .font(.system(size: 8))
                            .foregroundStyle(.red)
                            .offset(x: 3, y: -3)
                    }
                }
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Folders")
    }

    /// One root's entry. Its scan, when it is saying anything, rides the
    /// entry's icon; a failed walk carries what went wrong as the entry's
    /// tooltip, which is where the header used to spell it out.
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
                    .foregroundStyle(.red)
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

    /// How a folder on a network volume is kept up to date, in the user's
    /// language. `nil` for a folder on this machine's own disk, which its watch
    /// reports the moment it changes and which has nothing to explain.
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
            onAddFolder: {},
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
        .padding()
        .windowBackground()
    }
#endif
