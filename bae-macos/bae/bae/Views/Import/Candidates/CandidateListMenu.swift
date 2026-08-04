import BaeKit
import SwiftUI

/// The candidate list's one menu — an ellipsis button holding everything that
/// acts on the list rather than on a row: the sort order, and the watched
/// folders the list is built from.
///
/// One control rather than two. A separate "+" button advertised adding a
/// folder as a peer of sorting, when it is one item in a menu that also
/// refreshes, reveals and removes the roots already being watched.
struct CandidateListMenu: View {
    @Binding
    var sortOrder: CandidateSortOrder
    let watchedFolders: [BridgeWatchedFolder]
    /// The roots with a refresh in flight — their entry says so and cannot be
    /// asked again.
    let refreshingFolders: Set<String>
    let onAddFolder: () -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Stop watching `path`. Release grouping belongs to the queue below;
    /// removing a root stays an action here.
    let onRemoveFolder: (_ path: String) -> Void

    var body: some View {
        Menu {
            Section("Sort") {
                sortButton(.nameAZ, "Name (A\u{2013}Z)")
                sortButton(.nameZA, "Name (Z\u{2013}A)")
            }
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
        .help("Sorting and watched folders")
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

    @ViewBuilder
    private func sortButton(
        _ order: CandidateSortOrder,
        _ label: LocalizedStringKey
    ) -> some View {
        Button {
            sortOrder = order
        } label: {
            if sortOrder == order {
                Label(label, systemImage: "checkmark")
            }
            else {
                Text(label)
            }
        }
    }
}

#if DEBUG
    #Preview("Candidate list menu") {
        @Previewable
        @State
        var order: CandidateSortOrder = .nameAZ
        CandidateListMenu(
            sortOrder: $order,
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
