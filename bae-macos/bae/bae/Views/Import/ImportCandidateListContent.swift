import BaeKit
import SwiftUI

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

    @Environment(UiStore.self)
    private var uiStore
    @AppStorage("importCandidateSort")
    private var sortOrder: CandidateSortOrder = .dateAddedNewest

    private var filterTextBinding: Binding<String> {
        Binding(
            get: { uiStore.importCandidateFilterText },
            set: { uiStore.setImportCandidateFilterText($0) }
        )
    }

    private var activeTabBinding: Binding<CandidateTab> {
        Binding(
            get: { uiStore.importCandidateTab },
            set: { uiStore.setImportCandidateTab($0) }
        )
    }

    /// One section per watched folder, candidates in the active tab, filtered and
    /// sorted — built by the store. The active tab and filter text are
    /// UI-originated session state on `UiStore`; `sortOrder` is
    /// `AppStorage`-backed. Used for the New and Added tabs; the Skipped tab
    /// uses `displayedSkippedGroups` (it also holds invalid rows).
    private var displayedGroups:
        [(folder: BridgeWatchedFolder, candidates: [Candidate])]
    {
        importStore.candidateGroups(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    /// One section per watched folder for the Skipped tab: manually-skipped
    /// candidates plus invalid folders, built by the store.
    private var displayedSkippedGroups:
        [(folder: BridgeWatchedFolder, rows: [SkippedRow])]
    {
        importStore.skippedGroups(
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    /// True when the active tab has nothing to show — drives the empty state.
    private var activeTabIsEmpty: Bool {
        uiStore.importCandidateTab == .skipped
            ? displayedSkippedGroups.isEmpty : displayedGroups.isEmpty
    }

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 8) {
                CandidateTabBar(
                    activeTab: activeTabBinding,
                    importStore: importStore
                )
                HStack(spacing: 6) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(.tertiary)
                        .font(.callout)
                    TextField("Filter...", text: filterTextBinding)
                        .textFieldStyle(.plain)
                        .font(.callout)
                    if !uiStore.importCandidateFilterText.isEmpty {
                        Button {
                            uiStore.setImportCandidateFilterText("")
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
            if activeTabIsEmpty {
                emptyState
            }
            else if uiStore.importCandidateTab == .skipped {
                skippedList
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
            uiStore.importCandidateFilterText.isEmpty
                ? "Nothing here yet" : "No matches",
            systemImage: uiStore.importCandidateFilterText.isEmpty
                ? emptyTabSymbol : "magnifyingglass"
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface)
    }

    private var emptyTabSymbol: String {
        switch uiStore.importCandidateTab {
        case .new: "folder"
        case .added: "checkmark.circle"
        case .skipped: "minus.circle"
        }
    }

    private var candidateList: some View {
        List(selection: $selectedKey) {
            ForEach(displayedGroups, id: \.folder.path) { group in
                Section {
                    if !uiStore.collapsedImportFolders.contains(
                        group.folder.path
                    ) {
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
                        isCollapsed: uiStore.collapsedImportFolders.contains(
                            group.folder.path
                        ),
                        onToggle: {
                            uiStore.toggleImportFolderCollapsed(
                                group.folder.path
                            )
                        },
                        onRemove: { onRemoveFolder(group.folder.path) },
                    )
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    /// The Skipped tab: manually-skipped candidates and invalid folders, grouped
    /// by watched folder. Invalid rows aren't selectable — selecting their key is
    /// a no-op upstream (they aren't in `folderCandidates`, so no detail pane
    /// opens).
    private var skippedList: some View {
        List(selection: $selectedKey) {
            ForEach(displayedSkippedGroups, id: \.folder.path) { group in
                Section {
                    if !uiStore.collapsedImportFolders.contains(
                        group.folder.path
                    ) {
                        ForEach(group.rows) { row in
                            skippedRow(row)
                        }
                    }
                } header: {
                    FolderSectionHeader(
                        folder: group.folder,
                        count: group.rows.count,
                        isCollapsed: uiStore.collapsedImportFolders.contains(
                            group.folder.path
                        ),
                        onToggle: {
                            uiStore.toggleImportFolderCollapsed(
                                group.folder.path
                            )
                        },
                        onRemove: { onRemoveFolder(group.folder.path) },
                    )
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    @ViewBuilder
    private func skippedRow(_ row: SkippedRow) -> some View {
        switch row {
        case .candidate(let candidate):
            CandidateRow(
                displayName: candidate.displayName,
                revealPath: candidate.folderPathIfReal,
                status: candidate.importStatus,
                isAdded: candidate.isAdded,
                skipped: candidate.skipped,
                isLikelyDupe: isLikelyDupe(candidate.displayName),
                onSkip: { skipped in onSkip(candidate.key, skipped) },
            )
            .tag(candidate.key)
        case .invalid(let invalid):
            InvalidCandidateRow(
                displayName: invalid.sourceFolderName,
                reason: invalid.reason,
                revealPath: invalid.folderPath,
            )
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
        for invalid in PreviewData.invalidCandidates {
            store.invalidCandidates[invalid.folderPath] = invalid
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
        .environment(UiStore())
        .frame(width: 280, height: 500)
        .windowBackground()
    }
#endif
