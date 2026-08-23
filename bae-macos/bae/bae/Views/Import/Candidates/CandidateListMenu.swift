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
            && lhs.scanStatuses == rhs.scanStatuses
    }

    private var hasFailedScan: Bool {
        scanStatuses.values.contains { status in
            if case .failed = status { return true }
            return false
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
        switch scanStatuses[folder.path] {
        case .scanning:
            folderEntry(folder) {
                ProgressView().controlSize(.small)
            }
        case .failed(let error):
            folderEntry(folder) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
            }
            .help(error)
        case .complete, nil:
            folderEntry(folder) { EmptyView() }
        }
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
                "/Music/Rips": .scanning,
                "/Volumes/Vault": .failed(
                    error: "The volume could not be reached."
                ),
            ],
            onAddFolder: {},
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
        .padding()
        .windowBackground()
    }
#endif
