import BaeKit
import Foundation

@testable import bae

/// One import-tab shot: the store both halves of the tab read, and the
/// list items each tab holds. A canvas asks for the slot its `UiStore`'s
/// tab names, which is what a live list would have delivered.
struct ImportPreviewFixture {
    let store: ImportStore
    let itemsByTab: [BridgeTriageTab: [BridgeImportListItem]]

    @MainActor
    func slot(uiStore: UiStore) -> ImportListSlot {
        ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: itemsByTab[uiStore.importCandidateTab] ?? []
        )
    }
}

/// The list items and summaries the import shots are built from. Core
/// computes the stable keys in production; these mirror the same shapes so
/// a canvas addresses its rows the way the app does.
extension PreviewData {
    static func candidateItem(
        _ row: BridgeTriageRow
    ) -> BridgeImportListItem {
        candidateItem(row, headerKey: nil)
    }

    /// `row` listed under the group header with the stable key `headerKey`,
    /// or in no group.
    static func candidateItem(
        _ row: BridgeTriageRow,
        headerKey: String?
    ) -> BridgeImportListItem {
        .candidate(
            stableKey: "candidate:\(hex(row.candidateKey))",
            row: row,
            headerKey: headerKey
        )
    }

    static func invalidItem(
        _ candidate: BridgeInvalidCandidate
    ) -> BridgeImportListItem {
        .invalid(
            stableKey: "invalid:\(hex(candidate.folderPath))",
            invalidCandidate: candidate
        )
    }

    /// The stable key core gives the header of the group `key` names.
    static func groupHeaderStableKey(
        _ key: BridgeFolderReleaseDecisionKey
    ) -> String {
        "group:\(hex(key.watchedFolderPath)):\(hex(key.relativeFolder))"
    }

    static func groupHeaderItem(
        key: BridgeFolderReleaseDecisionKey,
        name: String,
        expanded: Bool = true,
        combinable: Bool = false,
        entryCount: UInt32
    ) -> BridgeImportListItem {
        .groupHeader(
            stableKey: groupHeaderStableKey(key),
            group: BridgeTriageGroup(
                key: key,
                name: name,
                combinable: combinable
            ),
            watchedFolderPath: key.watchedFolderPath,
            expanded: expanded,
            entryCount: entryCount
        )
    }

    /// Found's filter entries as core lists them, with made-up counts;
    /// In Progress holds nothing, so it cannot be chosen.
    static func pendingFilterEntries() -> [BridgePendingFilterEntry] {
        [
            (BridgePendingFilter.all, 42),
            (.needsYou, 12),
            (.inProgress, 0),
            (.identified, 21),
            (.unmatched, 2),
            (.notLookedUp, 7),
        ]
        .map { filter, count in
            BridgePendingFilterEntry(
                filter: filter,
                count: UInt32(count),
                selectable: count > 0
            )
        }
    }

    /// The tab counts a shot's tab bar reads, as the stream core feeds
    /// holds them.
    static func tabCounts(
        pending: UInt32,
        done: UInt32,
        skipped: UInt32
    ) -> LatestStream<UInt32> {
        .tabCounts(holding: [
            BridgeTriageTab.pending.countKey: pending,
            BridgeTriageTab.done.countKey: done,
            BridgeTriageTab.skipped.countKey: skipped,
        ])
    }

    static func importQueueSummary(
        hasWatchedFolders: Bool,
        hasGroups: Bool = false,
        filterText: String = ""
    ) -> BridgeImportQueueSummary {
        BridgeImportQueueSummary(
            hasWatchedFolders: hasWatchedFolders,
            hasGroups: hasGroups,
            narrowing: BridgeImportListNarrowing(
                tab: .pending,
                filterText: filterText,
                pendingFilter: .all
            )
        )
    }

    /// A selection of `keys`, with what it offers as core's selection
    /// statement answers it: each
    /// action offered once per selected candidate whose row offers it, in the
    /// order the surfaces list them. The fixture offers Combine when the
    /// person selects two or more rows, counting them and enabled when
    /// `combinable` says every row can join one release, and counting none
    /// otherwise. This function repeats core's selection query for fixtures,
    /// because the apps have no copy of that query.
    @MainActor
    static func importSelection(
        of keys: [Data],
        in store: ImportStore,
        combinable: Bool = false
    ) -> ImportSelection {
        let listed: [[BridgeCandidateAction]] = keys.map {
            store.selectedCandidates[$0]?.live?.offers.map(\.action) ?? []
        }
        let offers = selectionActionOrder.compactMap {
            action -> BridgeSelectionOffer? in
            if action == .combine {
                return keys.count >= 2
                    ? BridgeSelectionOffer(
                        action: action,
                        count: combinable ? UInt64(keys.count) : 0,
                        enabled: combinable
                    ) : nil
            }
            let count = listed.filter { $0.contains(action) }.count
            return count > 0
                ? BridgeSelectionOffer(
                    action: action,
                    count: UInt64(count),
                    enabled: true
                ) : nil
        }
        let selection = ImportSelection()
        selection.select(Set(keys))
        selection.apply(
            BridgeSelectionSummary(
                count: UInt64(keys.count),
                single: keys.count == 1
                    ? keys.first.map { .candidate(key: $0) } : nil,
                offers: offers
            )
        )
        return selection
    }

    /// The order every surface lists an action in.
    private static let selectionActionOrder: [BridgeCandidateAction] = [
        .import, .cancelImport, .identify, .cancelIdentification,
        .retryIdentification, .resetToFileMetadata, .clearMetadata, .combine,
        .separate, .skip, .restore, .revealFolder,
    ]

    /// `bytes` as lowercase hex, the way core spells bytes in a list key.
    private static func hex(_ bytes: Data) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}
