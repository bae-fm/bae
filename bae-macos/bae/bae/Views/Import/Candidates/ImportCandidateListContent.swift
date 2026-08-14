import BaeKit
import SwiftUI

func releaseGroupDisclosureID(
    _ key: BridgeFolderReleaseDecisionKey
) -> ReleaseGroupDisclosureID {
    ReleaseGroupDisclosureID(key: key)
}

// MARK: - ImportCandidateListContent

/// The import sidebar: three tabs over one triage queue. Every row, its tab,
/// its Pending group, and the tab counts come from
/// `ImportStore.triageQueue` — core's projection — read through
/// `ImportStore`'s filtering helpers. This view iterates and renders;
/// it decides nothing about where a row belongs.
struct ImportCandidateListContent: View {
    /// Read at the leaf: the triage queue and its grouping/filtering
    /// come from the store.
    let importStore: ImportStore
    @Binding
    var selectedKey: String?
    let onAddFolder: () -> Void
    /// Stop watching `path`. Reached from the watched-folders menu; release
    /// groups inside each root are rendered by the queue sections below.
    let onRemoveFolder: (_ path: String) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void
    /// Skip (or unskip) the candidate at `key`. Wired to the row context menu.
    let onSkip: (_ key: String, _ skipped: Bool) -> Void
    /// Import every candidate in `keys` — Pending's bulk action.
    let onImportSelected: (_ keys: [String]) -> Void

    @Environment(UiStore.self)
    private var uiStore
    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
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
        importStore.selectableReadyRows(
            filterText: uiStore.importCandidateFilterText
        )
    }

    private func sections(_ tab: BridgeTriageTab) -> [ReleaseQueueSection] {
        importStore.releaseSections(
            tab: tab,
            filterText: uiStore.importCandidateFilterText
        )
    }

    private var releaseGroupDisclosureIDs: Set<ReleaseGroupDisclosureID> {
        Set(
            importStore.triageQueue.sections.compactMap { section in
                section.group.map {
                    releaseGroupDisclosureID($0.key)
                }
            }
        )
    }

    /// True when the active tab has nothing to show — drives the empty state.
    private var activeTabIsEmpty: Bool {
        switch uiStore.importCandidateTab {
        case .pending, .done, .skipped:
            sections(uiStore.importCandidateTab).isEmpty
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
                    CandidateListMenu(
                        watchedFolders: importStore.watchedFolders,
                        refreshingFolders: uiStore.refreshingWatchedFolders,
                        onAddFolder: onAddFolder,
                        onRefreshFolder: onRefreshFolder,
                        onRemoveFolder: onRemoveFolder
                    )
                }
                .padding(.horizontal, 14)
                .padding(.bottom, 10)

                if let progress = importStore.queueIdentifyProgress,
                    progress.total > 0
                {
                    QueueProgressView(
                        identified: progress.identified,
                        total: progress.total,
                        onGoToUnidentified: importStore
                            .firstUnidentifiedCandidateKey
                            .map { key in
                                {
                                    uiStore.setImportCandidateTab(.pending)
                                    selectedKey = key
                                }
                            }
                    )
                    .padding(.horizontal, 14)
                    .padding(.bottom, 12)
                }

                folderScanStatuses
            }
        } content: {
            if activeTabIsEmpty {
                emptyState
            }
            else {
                tabList
            }
        }
        .task(id: releaseGroupDisclosureIDs) {
            uiStore.retainReleaseGroupDisclosureIDs(
                releaseGroupDisclosureIDs
            )
        }
        // Importable rows are covers this app has already downloaded once.
        // Decoding them as the queue lands keeps Pending's first frame from
        // being a grid of spinners.
        .task(id: importStore.readyCoverThumbnailUrls) {
            await imageStore.warm(
                importStore.readyCoverThumbnailUrls.map {
                    .remote(url: $0)
                },
                pointSize: TriageRowView.coverPointSize,
                displayScale: displayScale
            )
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
        case .pending: "questionmark.circle"
        case .done: "tray.full"
        case .skipped: "minus.circle"
        }
    }

    @ViewBuilder
    private var tabList: some View {
        switch uiStore.importCandidateTab {
        case .pending: pendingList
        case .done: doneList
        case .skipped: skippedList
        }
    }
}

struct FolderReleaseBoundaryRow: View {
    let boundary: BridgeFolderReleaseBoundary
    let onDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                boundary.name,
                systemImage: "folder.badge.questionmark"
            )
            .font(.system(size: 14, weight: .semibold))
            VStack(alignment: .leading, spacing: 3) {
                Text(boundary.displayPath)
                    .font(.system(size: 11.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                if boundary.sharedFileCount > 0 {
                    Label(
                        "\(Int64(boundary.sharedFileCount)) files",
                        systemImage: "doc.on.doc"
                    )
                    .foregroundStyle(.secondary)
                }
                ForEach(boundary.treeRows, id: \.displayPath) { row in
                    folderReleaseTreeRow(row)
                        .padding(.leading, CGFloat(row.depth) * 12)
                        .contextMenu {
                            releaseDecisionMenu(row.decisionKey)
                        }
                }
            }
            .font(.system(size: 11.5))
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 8) {
                    decisionButtons
                }
                VStack(alignment: .leading, spacing: 6) {
                    decisionButtons
                }
            }
            .controlSize(.small)
        }
        .padding(.vertical, 8)
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder
    private var decisionButtons: some View {
        Button("Combine as One Release") {
            onDecision(boundary.key, .combineAsOneRelease)
        }
        Button("Keep as Separate Releases") {
            onDecision(boundary.key, .keepAsSeparateReleases)
        }
    }

    private func folderReleaseTreeRow(
        _ row: BridgeFolderReleaseTreeRow
    ) -> some View {
        HStack(spacing: 5) {
            switch row.kind {
            case .folder:
                Image(systemName: "folder")
                Text(row.name)
            case .candidate(let trackCount, let formatLabel):
                Image(systemName: "opticaldisc")
                Text(row.name)
                Spacer(minLength: 4)
                Text(verbatim: "\(Int(trackCount)) \u{b7} \(formatLabel)")
                    .foregroundStyle(.secondary)
            case .invalid(let reason):
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                Text(row.name)
                Spacer(minLength: 4)
                Text(reason.localizedText)
                    .foregroundStyle(.red)
            }
        }
    }

    @ViewBuilder
    private func releaseDecisionMenu(
        _ key: BridgeFolderReleaseDecisionKey
    ) -> some View {
        Button("Combine as One Release") {
            onDecision(key, .combineAsOneRelease)
        }
        Button("Keep as Separate Releases") {
            onDecision(key, .keepAsSeparateReleases)
        }
    }
}

/// The watched-folder scan lines under the header: one per root that is
/// being walked or failed to walk, each with its own retry.
extension ImportCandidateListContent {
    private var folderScanStatuses: some View {
        ForEach(
            importStore.triageQueue.folderScanStatuses,
            id: \.watchedFolderPath
        ) { scan in
            switch scan.status {
            case .scanning:
                folderScanStatus(
                    scan,
                    icon: "arrow.clockwise",
                    text: String(localized: "Scanning\u{2026}"),
                    tint: .secondary
                )
            case .failed(let error):
                folderScanStatus(
                    scan,
                    icon: "exclamationmark.triangle.fill",
                    text: error,
                    tint: .red
                )
            case .complete:
                EmptyView()
            }
        }
    }

    private func folderScanStatus(
        _ scan: BridgeWatchedFolderScanStatus,
        icon: String,
        text: String,
        tint: Color
    ) -> some View {
        let refreshing = uiStore.refreshingWatchedFolders
            .contains(scan.watchedFolderPath)
        return HStack(alignment: .firstTextBaseline, spacing: 7) {
            Image(systemName: icon)
            Text(
                verbatim:
                    "\(scan.watchedFolderName) (\(scan.watchedFolderPath)): \(text)"
            )
            .lineLimit(2)
            .truncationMode(.middle)
            Spacer(minLength: 4)
            Button(refreshing ? "Refreshing\u{2026}" : "Refresh") {
                onRefreshFolder(
                    BridgeWatchedFolder(
                        path: scan.watchedFolderPath,
                        name: scan.watchedFolderName
                    )
                )
            }
            .disabled(refreshing)
            .controlSize(.small)
        }
        .font(.system(size: 11.5))
        .foregroundStyle(tint)
        .padding(.horizontal, 14)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The per-tab lists. In an extension so the view's body and the chrome it
/// builds — tabs, filter, progress — read as one piece above them.
extension ImportCandidateListContent {
    // MARK: - Pending

    private var selectedReadyKeys: Set<String> {
        let currentReady = Set(readyRows.map(\.candidateKey))
        return uiStore.selectedReadyCandidates.intersection(currentReady)
    }

    private var pendingList: some View {
        VStack(spacing: 0) {
            List(selection: $selectedKey) {
                ForEach(sections(.pending)) { section in
                    releaseSection(section)
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

    // MARK: - Done

    private var doneList: some View {
        List(selection: $selectedKey) {
            ForEach(sections(.done)) { section in
                releaseSection(section)
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
            ForEach(sections(.skipped)) { section in
                releaseSection(section)
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    /// A folder group renders as a header row followed by its rows as
    /// siblings — not a `DisclosureGroup`, which indents whatever it contains.
    /// Grouped and ungrouped rows share one leading edge; the header row is
    /// the only thing that says a group is there.
    @ViewBuilder
    private func releaseSection(_ section: ReleaseQueueSection) -> some View {
        if let group = section.group {
            let disclosureID = releaseGroupDisclosureID(group.key)
            let expanded = uiStore.releaseGroupExpanded(disclosureID)
            releaseGroupHeader(group, expanded: expanded, id: disclosureID)
            if expanded {
                ForEach(section.entries) { entry in
                    releaseEntry(entry.bridge)
                }
            }
        }
        else {
            ForEach(section.entries) { entry in
                releaseEntry(entry.bridge)
            }
        }
    }

    private func releaseGroupHeader(
        _ group: BridgeTriageGroup,
        expanded: Bool,
        id: ReleaseGroupDisclosureID
    ) -> some View {
        Button {
            uiStore.setReleaseGroupExpanded(id, !expanded)
        } label: {
            HStack(spacing: 6) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 9)
                Label(group.name, systemImage: "folder")
                    .font(.system(size: 12.5, weight: .semibold))
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Combine as One Release") {
                onReleaseDecision(group.key, .combineAsOneRelease)
            }
            Button("Keep as Separate Releases") {
                onReleaseDecision(group.key, .keepAsSeparateReleases)
            }
        }
    }

    @ViewBuilder
    private func releaseEntry(_ entry: BridgeTriageEntry) -> some View {
        switch entry {
        case .candidate(_, let row):
            TriageRowView(
                row: row,
                selection: row.selectable
                    ? (
                        isSelected: uiStore.selectedReadyCandidates
                            .contains(row.candidateKey),
                        toggle: {
                            uiStore.toggleReadySelection(row.candidateKey)
                        }
                    ) : nil,
                onSelect: { selectedKey = row.candidateKey },
                onSkip: { onSkip(row.candidateKey, $0) },
                onReleaseDecision: onReleaseDecision
            )
            .tag(row.candidateKey)
            .disabled(!row.actionable)
        case .boundary(_, let boundary):
            FolderReleaseBoundaryRow(
                boundary: boundary,
                onDecision: onReleaseDecision
            )
        case .invalid(_, let invalid):
            InvalidCandidateRow(
                displayName: invalid.sourceFolderName,
                reason: invalid.reason,
                revealPath: invalid.folderPath
            )
            .contextMenu {
                ForEach(
                    invalid.resolvedBoundaries,
                    id: \.key
                ) { boundary in
                    switch boundary.decision {
                    case .combineAsOneRelease:
                        Button("Keep as Separate Releases") {
                            onReleaseDecision(
                                boundary.key,
                                .keepAsSeparateReleases
                            )
                        }
                    case .keepAsSeparateReleases:
                        Button("Combine as One Release") {
                            onReleaseDecision(
                                boundary.key,
                                .combineAsOneRelease
                            )
                        }
                    }
                }
            }
        }
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Candidate List") {
        let store = PreviewData.importTabStore()
        return ImportCandidateListContent(
            importStore: store,
            selectedKey: .constant(
                store.selectableReadyRows(
                    filterText: ""
                )
                .first?
                .candidateKey
            ),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(UiStore())
        .environment(PreviewData.artImageStore())
        .frame(width: 340, height: 560)
        .windowBackground()
    }

    #Preview("Candidate List Narrow") {
        let store = PreviewData.importTabStore()
        let uiStore = UiStore()
        uiStore.setReleaseGroupExpanded(
            releaseGroupDisclosureID(
                PreviewData.folderReleaseBoundary.key
            ),
            false
        )
        uiStore.setWatchedFolderRefreshing(
            PreviewData.importWatchedFolder.path,
            true
        )
        return ImportCandidateListContent(
            importStore: store,
            selectedKey: .constant(nil),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .frame(width: 280, height: 560)
        .windowBackground()
    }

    #Preview("Release Boundary") {
        FolderReleaseBoundaryRow(
            boundary: PreviewData.folderReleaseBoundary,
            onDecision: { _, _ in }
        )
        .frame(width: 320)
        .padding()
        .windowBackground()
    }

    #Preview("Release Boundaries — Mixed Trees") {
        ImportCandidateListContent(
            importStore: PreviewData.releaseBoundaryPreviewImportStore,
            selectedKey: .constant(nil),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(UiStore())
        .environment(PreviewData.artImageStore())
        .frame(width: 520, height: 900)
        .windowBackground()
    }
#endif
