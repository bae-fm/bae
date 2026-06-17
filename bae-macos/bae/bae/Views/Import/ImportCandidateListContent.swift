import SwiftUI

// MARK: - Shared sidebar layout

private struct ImportSidebarList<Header: View, Content: View>: View {
    @ViewBuilder
    let header: () -> Header
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                header()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(Theme.surface)
            Divider()
            content()
        }
    }
}

// MARK: - ImportCandidateListContent

/// The import candidate list, grouped by the watched folder each candidate was
/// scanned from. One collapsible section per folder, in the order the folders
/// were added.
struct ImportCandidateListContent: View {
    /// Read at the leaf: the watched folders and candidates, and the grouping /
    /// filtering / sorting, all come from the store.
    let importStore: ImportStore
    @Binding
    var selectedKey: String?
    let isLikelyDupe: (String) -> Bool
    let onAddFolder: () -> Void
    let onRemoveFolder: (_ path: String) -> Void

    @State
    private var filterText = ""
    @State
    private var collapsedFolders: Set<String> = []
    @AppStorage("importCandidateSort")
    private var sortOrder: CandidateSortOrder = .dateAddedNewest

    /// One section per watched folder, candidates filtered and sorted — built by
    /// the store. `filterText` and `sortOrder` are this view's own UI state.
    private var displayedGroups:
        [(folder: BridgeWatchedFolder, candidates: [Candidate])]
    {
        importStore.candidateGroups(
            filterText: filterText,
            sortOrder: sortOrder
        )
    }

    var body: some View {
        ImportSidebarList {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.tertiary)
                    .font(.callout)
                TextField("Filter...", text: $filterText)
                    .textFieldStyle(.plain)
                    .font(.callout)
                if !filterText.isEmpty {
                    Button {
                        filterText = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.tertiary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: .infinity)
            sortMenu
            Button(action: onAddFolder) {
                Image(systemName: "plus")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("Add a folder to watch for imports")
        } content: {
            List(selection: $selectedKey) {
                ForEach(displayedGroups, id: \.folder.path) { group in
                    Section {
                        if !collapsedFolders.contains(group.folder.path) {
                            ForEach(group.candidates, id: \.key) { candidate in
                                CandidateRow(
                                    displayName: candidate.displayName,
                                    revealPath: candidate.folderPathIfReal,
                                    status: candidate.importStatus,
                                    isLikelyDupe: isLikelyDupe(
                                        candidate.displayName
                                    ),
                                )
                                .tag(candidate.key)
                            }
                        }
                    } header: {
                        FolderSectionHeader(
                            folder: group.folder,
                            count: group.candidates.count,
                            isCollapsed: collapsedFolders.contains(
                                group.folder.path
                            ),
                            onToggle: { toggleCollapsed(group.folder.path) },
                            onRemove: { onRemoveFolder(group.folder.path) },
                        )
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.surface)
        }
    }

    private func toggleCollapsed(_ path: String) {
        if collapsedFolders.contains(path) {
            collapsedFolders.remove(path)
        }
        else {
            collapsedFolders.insert(path)
        }
    }

    private var sortMenu: some View {
        Menu {
            Section("Sort") {
                sortButton(.nameAZ, "Name (A\u{2013}Z)")
                sortButton(.nameZA, "Name (Z\u{2013}A)")
                sortButton(.dateAddedNewest, "Date Added (Newest)")
                sortButton(.dateAddedOldest, "Date Added (Oldest)")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private func sortButton(_ order: CandidateSortOrder, _ label: String)
        -> some View
    {
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

// MARK: - FolderSectionHeader

private struct FolderSectionHeader: View {
    let folder: BridgeWatchedFolder
    let count: Int
    let isCollapsed: Bool
    let onToggle: () -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)
                .rotationEffect(.degrees(isCollapsed ? 0 : 90))
            Image(systemName: "folder")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(folder.name)
                .font(.caption)
                .fontWeight(.semibold)
                .textCase(.uppercase)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            Text("\(count)")
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onToggle)
        .help(folder.path)
        .contextMenu {
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: folder.path)
            }
            Divider()
            Button("Remove Folder", role: .destructive, action: onRemove)
        }
    }
}

// MARK: - CandidateRow

struct CandidateRow: View {
    let displayName: String
    /// Real filesystem path for Reveal in Finder. Always set for folder
    /// candidates.
    let revealPath: String?
    let status: ImportStatus?
    let isLikelyDupe: Bool

    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        HStack(spacing: 8) {
            statusIcon
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(displayName)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if isLikelyDupe, status == nil {
                    Text("Likely imported")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .opacity(isLikelyDupe ? 0.6 : 1.0)
            Spacer()
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .contextMenu {
            if let revealPath {
                Button("Reveal in Finder") {
                    SystemActions.revealInFinder(path: revealPath)
                }
            }
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let status {
            switch status {
            case .importing:
                ProgressView()
                    .controlSize(.small)
            case .complete(_, let releaseId):
                // Imported, but the cloud upload may still be draining — show
                // the transfer until the release's queue empties.
                if let progress = outboxStore.progress(forRelease: releaseId),
                    !progress.isIdle
                {
                    Image(systemName: "icloud.and.arrow.up")
                        .foregroundStyle(.secondary)
                }
                else {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                }
            case .error:
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
            }
        }
        else {
            Image(systemName: "folder")
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Previews

#Preview("Candidate List") {
    let store = ImportStore()
    store.watchedFolders = [
        BridgeWatchedFolder(path: "/Music/Downloads", name: "Downloads"),
        BridgeWatchedFolder(path: "/Volumes/Rips", name: "Rips"),
    ]
    for candidate in PreviewData.folderCandidates {
        var copy = candidate
        copy.importStatus = PreviewData.importStatuses[candidate.key]
        store.folderCandidates[copy.key] = copy
    }
    return ImportCandidateListContent(
        importStore: store,
        selectedKey: .constant(store.folderCandidates.keys.first),
        isLikelyDupe: { $0 == "Album Title One" },
        onAddFolder: {},
        onRemoveFolder: { _ in },
    )
    .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    .frame(width: 280, height: 500)
}
