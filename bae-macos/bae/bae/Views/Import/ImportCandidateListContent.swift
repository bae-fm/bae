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
    /// Skip (or unskip) the candidate at `key`. Wired to the row context menu.
    let onSkip: (_ key: String, _ skipped: Bool) -> Void

    @State
    private var activeTab: CandidateTab = .new
    @State
    private var filterText = ""
    @State
    private var collapsedFolders: Set<String> = []
    @AppStorage("importCandidateSort")
    private var sortOrder: CandidateSortOrder = .dateAddedNewest

    /// One section per watched folder, candidates in the active tab, filtered and
    /// sorted — built by the store. `activeTab`, `filterText`, and `sortOrder`
    /// are this view's own UI state.
    private var displayedGroups:
        [(folder: BridgeWatchedFolder, candidates: [Candidate])]
    {
        importStore.candidateGroups(
            tab: activeTab,
            filterText: filterText,
            sortOrder: sortOrder
        )
    }

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 8) {
                CandidateTabBar(
                    activeTab: $activeTab,
                    importStore: importStore
                )
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
                    sortMenu
                    Button(action: onAddFolder) {
                        Image(systemName: "plus")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help("Add a folder to watch for imports")
                }
            }
            .frame(maxWidth: .infinity)
        } content: {
            if displayedGroups.isEmpty {
                emptyState
            }
            else {
                candidateList
            }
        }
    }

    /// Per-tab empty state: distinguishes "no matches" while filtering from
    /// "nothing in this tab yet".
    private var emptyState: some View {
        ContentUnavailableView(
            filterText.isEmpty ? "Nothing here yet" : "No matches",
            systemImage: filterText.isEmpty
                ? emptyTabSymbol : "magnifyingglass"
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface)
    }

    private var emptyTabSymbol: String {
        switch activeTab {
        case .new: "folder"
        case .added: "checkmark.circle"
        case .skipped: "minus.circle"
        }
    }

    private var candidateList: some View {
        List(selection: $selectedKey) {
            ForEach(displayedGroups, id: \.folder.path) { group in
                Section {
                    if !collapsedFolders.contains(group.folder.path) {
                        ForEach(group.candidates, id: \.key) { candidate in
                            CandidateRow(
                                displayName: candidate.displayName,
                                revealPath: candidate.folderPathIfReal,
                                status: candidate.importStatus,
                                isAdded: candidate.isAdded,
                                skipped: candidate.skipped,
                                isLikelyDupe: isLikelyDupe(
                                    candidate.displayName
                                ),
                                onSkip: { skipped in
                                    onSkip(candidate.key, skipped)
                                },
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

// MARK: - CandidateTabBar

/// The New / Added / Skipped tab bar above the candidate list. Three equal
/// segments, each a label plus a count badge; the active one is highlighted.
private struct CandidateTabBar: View {
    @Binding
    var activeTab: CandidateTab
    /// Read the counts at the leaf so the parent isn't subscribed to every
    /// candidate change just to pass them down.
    let importStore: ImportStore

    var body: some View {
        let counts = importStore.candidateTabCounts()
        HStack(spacing: 4) {
            segment(.new, "New", counts.new)
            segment(.added, "Added", counts.added)
            segment(.skipped, "Skipped", counts.skipped)
        }
    }

    private func segment(_ tab: CandidateTab, _ label: String, _ count: Int)
        -> some View
    {
        let isActive = activeTab == tab
        return Button {
            activeTab = tab
        } label: {
            HStack(spacing: 4) {
                Text(label)
                    .font(.caption)
                    .fontWeight(isActive ? .semibold : .regular)
                Text("\(count)")
                    .font(.caption2)
                    .monospacedDigit()
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(
                        Capsule()
                            .fill(
                                isActive
                                    ? Color.accentColor.opacity(0.25)
                                    : Color.secondary.opacity(0.15)
                            )
                    )
            }
            .foregroundStyle(isActive ? Color.accentColor : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(
                        isActive
                            ? Color.accentColor.opacity(0.12) : Color.clear
                    )
            )
        }
        .buttonStyle(.plain)
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
    /// The candidate was already imported from this folder (content-hash match),
    /// shown the same as an in-session completed import.
    let isAdded: Bool
    /// The user manually skipped this candidate.
    let skipped: Bool
    let isLikelyDupe: Bool
    /// Skip (true) or unskip (false) this candidate.
    let onSkip: (_ skipped: Bool) -> Void

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
                if isLikelyDupe, status == nil, !isAdded, !skipped {
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
            Button(skipped ? "Move to New" : "Skip") { onSkip(!skipped) }
            if let revealPath {
                Divider()
                Button("Reveal in Finder") {
                    SystemActions.revealInFinder(path: revealPath)
                }
            }
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if skipped {
            icon("minus.circle", .secondary)
        }
        else if let status {
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
                    icon("icloud.and.arrow.up", .secondary)
                }
                else {
                    icon("checkmark.circle.fill", .green)
                }
            case .error:
                icon("exclamationmark.triangle.fill", .red)
            }
        }
        else if isAdded {
            // An already-imported folder (content-hash match) shows the same
            // green check as an in-session completed import.
            icon("checkmark.circle.fill", .green)
        }
        else {
            icon("folder", .secondary)
        }
    }

    private func icon<S: ShapeStyle>(_ systemName: String, _ style: S)
        -> some View
    {
        Image(systemName: systemName).foregroundStyle(style)
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
        onSkip: { _, _ in },
    )
    .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    .frame(width: 280, height: 500)
}
