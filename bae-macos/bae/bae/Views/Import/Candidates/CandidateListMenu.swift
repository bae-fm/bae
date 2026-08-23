import BaeKit
import SwiftUI

/// The watched folders the candidate list is built from: add a root, or
/// refresh, reveal, and remove one already being watched.
///
/// `Equatable` over the folders and their refresh state alone, and rendered
/// through `.equatable()`: the queue summary this is built from is
/// re-delivered on every verdict the sweep commits, and a `Menu` whose content
/// is rebuilt while it is open closes under the pointer. Comparing the values
/// the menu actually draws keeps it standing across those ticks.
struct CandidateListMenu: View, Equatable {
    let watchedFolders: [BridgeWatchedFolder]
    /// The roots with a refresh in flight — their entry says so and cannot be
    /// asked again.
    let refreshingFolders: Set<String>
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
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Folders")
    }

    private func folderMenu(_ folder: BridgeWatchedFolder) -> some View {
        Menu(folder.name) {
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
        }
    }
}

#if DEBUG
    #Preview("Candidate list menu") {
        CandidateListMenu(
            watchedFolders: [PreviewData.importWatchedFolder],
            refreshingFolders: [],
            onAddFolder: {},
            onRefreshFolder: { _ in },
            onRemoveFolder: { _ in }
        )
        .padding()
        .windowBackground()
    }
#endif
