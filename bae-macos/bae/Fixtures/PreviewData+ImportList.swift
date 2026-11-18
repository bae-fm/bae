import BaeKit
import Foundation

@testable import bae

/// One import-tab shot: the store both halves of the tab read, and the
/// list items each tab holds. A canvas asks for the slot its `UiStore`'s
/// tab names, which is what a live list would have delivered.
struct ImportPreviewFixture {
    let store: ImportStore
    let itemsByTab: [BridgeTriageTab: [BridgeImportListItem]]
    /// How many rows each tab holds, as core sends them with the import
    /// list's count.
    let tabCounts: BridgeTriageTabCounts

    @MainActor
    func slot(
        uiStore: UiStore,
        selection: ImportSelection = ImportSelection()
    ) -> ImportListSlot {
        ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: itemsByTab[uiStore.importCandidateTab] ?? [],
            tabCounts: tabCounts,
            selection: selection
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
        candidateItem(row, header: nil)
    }

    /// `row` listed under the group header `header`, or in no group.
    static func candidateItem(
        _ row: BridgeTriageRow,
        header: BridgeImportGroupHeader?
    ) -> BridgeImportListItem {
        .candidate(
            stableKey: "candidate:\(hex(row.candidateKey))",
            row: row,
            header: header
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

    /// The header of the group `key` names.
    static func groupHeader(
        key: BridgeFolderReleaseDecisionKey,
        name: String,
        expanded: Bool = true,
        combinable: Bool = false,
        entryCount: UInt32
    ) -> BridgeImportGroupHeader {
        BridgeImportGroupHeader(
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

    /// The header row of the group `key` names.
    static func groupHeaderItem(
        key: BridgeFolderReleaseDecisionKey,
        name: String,
        expanded: Bool = true,
        combinable: Bool = false,
        entryCount: UInt32
    ) -> BridgeImportListItem {
        .groupHeader(
            header: groupHeader(
                key: key,
                name: name,
                expanded: expanded,
                combinable: combinable,
                entryCount: entryCount
            )
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

    /// What core's selection statement answers for the Import tab's
    /// identified folder and its folder whose signals disagree, selected
    /// together: only the first offers Import, and both offer the rest. When
    /// `combinable` is true, every selected folder can join one release, so
    /// Combine counts both and is enabled. Otherwise Combine counts none and
    /// is disabled.
    static func importTabPairSelection(
        combinable: Bool = false
    ) -> BridgeSelectionSummary {
        BridgeSelectionSummary(
            count: 2,
            single: nil,
            offers: [
                BridgeSelectionOffer(action: .import, count: 1, enabled: true),
                BridgeSelectionOffer(
                    action: .resetToFileMetadata,
                    count: 2,
                    enabled: true
                ),
                BridgeSelectionOffer(
                    action: .clearMetadata,
                    count: 2,
                    enabled: true
                ),
                BridgeSelectionOffer(
                    action: .combine,
                    count: combinable ? 2 : 0,
                    enabled: combinable
                ),
                BridgeSelectionOffer(action: .skip, count: 2, enabled: true),
                BridgeSelectionOffer(
                    action: .revealFolder,
                    count: 2,
                    enabled: true
                ),
            ]
        )
    }

    /// The Import tab's identified folder and its folder whose signals
    /// disagree, selected together, holding `importTabPairSelection` for
    /// `combinable`.
    @MainActor
    static func selectedImportTabPair(
        combinable: Bool = false
    ) -> ImportSelection {
        let selection = ImportSelection()
        selection.select([
            importTabCandidate.key, importTabDisagreementCandidate.key,
        ])
        selection.apply(importTabPairSelection(combinable: combinable))
        return selection
    }

    /// `bytes` as lowercase hex, the way core spells bytes in a list key.
    private static func hex(_ bytes: Data) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}
