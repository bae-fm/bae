#if DEBUG
    import BaeKit
    import Foundation

    /// One import-tab preview: the store both halves of the tab read, and the
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

    /// The list items and summaries the import previews are built from. Core
    /// computes the stable keys in production; these mirror the same shapes so
    /// a canvas addresses its rows the way the app does.
    extension PreviewData {
        static func candidateItem(
            _ row: BridgeTriageRow
        ) -> BridgeImportListItem {
            candidateItem(row, headerPosition: nil)
        }

        /// `row` listed under the group header at `headerPosition`, or in
        /// no group.
        static func candidateItem(
            _ row: BridgeTriageRow,
            headerPosition: UInt64?
        ) -> BridgeImportListItem {
            .candidate(
                stableKey: "candidate:\(hex(row.candidateKey))",
                row: row,
                headerPosition: headerPosition
            )
        }

        static func invalidItem(
            _ candidate: BridgeInvalidCandidate
        ) -> BridgeImportListItem {
            .invalid(
                stableKey: "invalid:\(hex(candidate.folderPath))",
                invalidCandidate: candidate,
                selected: false
            )
        }

        static func groupHeaderItem(
            key: BridgeFolderReleaseDecisionKey,
            name: String,
            expanded: Bool = true,
            combinable: Bool = false,
            entryCount: UInt32
        ) -> BridgeImportListItem {
            .groupHeader(
                stableKey:
                    "group:\(hex(key.watchedFolderPath)):\(hex(key.relativeFolder))",
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
            watchedFolders: [BridgeWatchedFolder],
            hasGroups: Bool = false,
            narrowed: BridgeNarrowedCount? = nil,
            filterText: String = ""
        ) -> BridgeImportQueueSummary {
            BridgeImportQueueSummary(
                watchedFolders: watchedFolders,
                hasGroups: hasGroups,
                narrowed: narrowed,
                narrowing: BridgeImportListNarrowing(
                    tab: .pending,
                    filterText: filterText,
                    pendingFilter: .all
                ),
                openedAt: 0
            )
        }

        /// A selection of `keys` as core would summarize it: what each
        /// candidate in `store` offers now, joined into the selection's offers.
        @MainActor
        static func importSelection(
            of keys: [Data],
            in store: ImportStore
        ) -> ImportSelection {
            let members = keys.map { key in
                BridgeSelectionMember(
                    actions: store.selectedCandidates[key]?.live?.actions ?? []
                )
            }
            let selection = ImportSelection()
            selection.apply(
                BridgeSelectionSummary(
                    count: UInt64(keys.count),
                    single: keys.count == 1
                        ? keys.first.map { .candidate(key: $0) } : nil,
                    offers: bridgeCandidateSelectionOffers(members: members)
                )
            )
            return selection
        }

        /// `bytes` as lowercase hex, the way core spells bytes in a list key.
        private static func hex(_ bytes: Data) -> String {
            bytes.map { String(format: "%02x", $0) }.joined()
        }
    }
#endif
