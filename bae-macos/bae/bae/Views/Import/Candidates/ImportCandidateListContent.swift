import BaeKit
import SwiftUI

// MARK: - ImportCandidateListContent

/// The import sidebar: four tabs over one triage queue. Every row, its tab,
/// its Needs-you group, and the tab counts come from
/// `ImportStore.triageQueue` — core's projection — read through
/// `ImportStore`'s filtering/sorting helpers. This view iterates and renders;
/// it decides nothing about where a row belongs.
struct ImportCandidateListContent: View {
    /// Read at the leaf: the triage queue and its grouping/filtering/sorting
    /// come from the store.
    let importStore: ImportStore
    @Binding
    var selectedKey: String?
    let onAddFolder: () -> Void
    /// Stop watching `path`. Reached from the watched-folders menu next to
    /// Add — there is no more per-folder section header to hang it off, now
    /// that a row's folder is a subtitle rather than a grouping.
    let onRemoveFolder: (_ path: String) -> Void
    /// Skip (or unskip) the candidate at `key`. Wired to the row context menu.
    let onSkip: (_ key: String, _ skipped: Bool) -> Void
    /// Import every candidate in `keys` — the Ready foot bar's bulk action.
    let onImportSelected: (_ keys: [String]) -> Void

    @Environment(UiStore.self)
    private var uiStore
    @AppStorage("importCandidateSort")
    private var sortOrder: CandidateSortOrder = .nameAZ

    private var filterTextBinding: Binding<String> {
        Binding(
            get: { uiStore.importCandidateFilterText },
            set: { uiStore.setImportCandidateFilterText($0) }
        )
    }

    private var activeTabBinding: Binding<BridgeTriageTab> {
        Binding(
            get: { uiStore.importCandidateTab },
            set: { uiStore.setImportCandidateTab($0) }
        )
    }

    private var readyRows: [BridgeTriageRow] {
        importStore.rows(
            tab: .ready,
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    private var needsYouGroups:
        [(group: BridgeNeedsYouGroup, rows: [BridgeTriageRow])]
    {
        importStore.needsYouGroups(
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    private var doneRows: [BridgeTriageRow] {
        importStore.rows(
            tab: .done,
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    private var skippedRows: [SkippedRow] {
        importStore.skippedRows(
            filterText: uiStore.importCandidateFilterText,
            sortOrder: sortOrder
        )
    }

    /// True when the active tab has nothing to show — drives the empty state.
    private var activeTabIsEmpty: Bool {
        switch uiStore.importCandidateTab {
        case .ready: readyRows.isEmpty
        case .needsYou: needsYouGroups.isEmpty
        case .done: doneRows.isEmpty
        case .skipped: skippedRows.isEmpty
        }
    }

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 0) {
                TriageTabBar(
                    activeTab: activeTabBinding,
                    counts: importStore.triageQueue.counts
                )
                .padding(.horizontal, 10)
                .padding(.top, 10)
                .padding(.bottom, 8)

                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 13))
                        .foregroundStyle(.tertiary)
                    TextField("Filter...", text: filterTextBinding)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12.5))
                    if !uiStore.importCandidateFilterText.isEmpty {
                        Button {
                            uiStore.setImportCandidateFilterText("")
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.plain)
                    }
                    CandidateSortMenu(sortOrder: $sortOrder)
                    watchedFoldersMenu
                }
                .padding(.horizontal, 14)
                .padding(.bottom, 10)

                if let progress = importStore.queueIdentifyProgress,
                    progress.total > 0
                {
                    QueueProgressView(
                        identified: progress.identified,
                        total: progress.total
                    )
                    .padding(.horizontal, 14)
                    .padding(.bottom, 12)
                }
            }
        } content: {
            if activeTabIsEmpty {
                emptyState
            }
            else {
                tabList
            }
        }
    }

    /// The `+` control: add a folder, and — once at least one is watched —
    /// stop watching one. There is no more per-folder section header to hang
    /// "Remove Folder" off, now that a row's folder is a subtitle rather than
    /// a grouping.
    private var watchedFoldersMenu: some View {
        Menu {
            Button {
                onAddFolder()
            } label: {
                Label("Add a Folder\u{2026}", systemImage: "plus")
            }
            if !importStore.watchedFolders.isEmpty {
                Divider()
                ForEach(importStore.watchedFolders, id: \.path) { folder in
                    Menu(folder.name) {
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
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 13))
        }
        .menuIndicator(.hidden)
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .fixedSize()
        .help("Add a folder to watch for imports")
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
        case .ready: "checkmark.circle"
        case .needsYou: "questionmark.circle"
        case .done: "tray.full"
        case .skipped: "minus.circle"
        }
    }

    @ViewBuilder
    private var tabList: some View {
        switch uiStore.importCandidateTab {
        case .ready: readyList
        case .needsYou: needsYouList
        case .done: doneList
        case .skipped: skippedList
        }
    }

    // MARK: - Ready

    private var selectedReadyKeys: Set<String> {
        let currentReady = Set(readyRows.map(\.candidateKey))
        return uiStore.selectedReadyCandidates.intersection(currentReady)
    }

    private var readyList: some View {
        VStack(spacing: 0) {
            List(selection: $selectedKey) {
                ForEach(readyRows, id: \.candidateKey) { row in
                    TriageRowView(
                        row: row,
                        isFocused: selectedKey == row.candidateKey,
                        selection: (
                            isSelected: uiStore.selectedReadyCandidates
                                .contains(row.candidateKey),
                            toggle: {
                                uiStore.toggleReadySelection(row.candidateKey)
                            }
                        ),
                        onSelect: { selectedKey = row.candidateKey },
                        onSkip: { onSkip(row.candidateKey, $0) }
                    )
                    .tag(row.candidateKey)
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.surface)
            Divider()
            TriageFootBar(
                selectedCount: selectedReadyKeys.count,
                readyCount: readyRows.count,
                onSelectAll: {
                    uiStore.selectAllReady(readyRows.map(\.candidateKey))
                },
                onImport: { onImportSelected(Array(selectedReadyKeys)) }
            )
        }
    }

    // MARK: - Needs you

    private var needsYouList: some View {
        List(selection: $selectedKey) {
            ForEach(needsYouGroups, id: \.group) { entry in
                Section {
                    ForEach(entry.rows, id: \.candidateKey) { row in
                        TriageRowView(
                            row: row,
                            isFocused: selectedKey == row.candidateKey,
                            selection: nil,
                            onSelect: { selectedKey = row.candidateKey },
                            onSkip: { onSkip(row.candidateKey, $0) }
                        )
                        .tag(row.candidateKey)
                    }
                } header: {
                    NeedsYouGroupHeader(
                        group: entry.group,
                        count: entry.rows.count
                    )
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    // MARK: - Done

    private var doneList: some View {
        List(selection: $selectedKey) {
            ForEach(doneRows, id: \.candidateKey) { row in
                TriageRowView(
                    row: row,
                    isFocused: selectedKey == row.candidateKey,
                    selection: nil,
                    onSelect: { selectedKey = row.candidateKey },
                    onSkip: { onSkip(row.candidateKey, $0) }
                )
                .tag(row.candidateKey)
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    // MARK: - Skipped

    /// Manually-skipped candidates and invalid folders together — core hands
    /// both across in `triageQueue` and both share this one tab, so there is
    /// nothing to group them by. Invalid rows aren't selectable — selecting
    /// their key is a no-op upstream (they aren't a real candidate), so they
    /// carry no `.tag()`.
    private var skippedList: some View {
        List(selection: $selectedKey) {
            ForEach(skippedRows) { row in
                skippedRowView(row)
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    @ViewBuilder
    private func skippedRowView(_ row: SkippedRow) -> some View {
        switch row {
        case .candidate(let triageRow):
            TriageRowView(
                row: triageRow,
                isFocused: selectedKey == triageRow.candidateKey,
                selection: nil,
                onSelect: { selectedKey = triageRow.candidateKey },
                onSkip: { onSkip(triageRow.candidateKey, $0) }
            )
            .tag(triageRow.candidateKey)
        case .invalid(let invalid):
            InvalidCandidateRow(
                displayName: invalid.sourceFolderName,
                reason: invalid.reason,
                revealPath: invalid.folderPath
            )
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Candidate List") {
        let store = PreviewData.triageImportStore
        return ImportCandidateListContent(
            importStore: store,
            selectedKey: .constant(store.triageQueue.rows.first?.candidateKey),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(UiStore())
        .frame(width: 340, height: 560)
        .windowBackground()
    }
#endif
