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

        /// This fixture with Found's first group folded shut, as core
        /// delivers a folded group: its header closed and its members gone.
        func foldingFirstGroup() -> ImportPreviewFixture {
            var pending: [BridgeImportListItem] = []
            var folding = false
            var folded = false
            for item in itemsByTab[.pending] ?? [] {
                switch item {
                case .groupHeader(
                    let stableKey,
                    let group,
                    let watchedFolderPath,
                    _,
                    let entryCount
                ) where !folded:
                    folded = true
                    folding = true
                    pending.append(
                        .groupHeader(
                            stableKey: stableKey,
                            group: group,
                            watchedFolderPath: watchedFolderPath,
                            expanded: false,
                            entryCount: entryCount
                        )
                    )
                    continue
                case .candidate(_, _, true) where folding,
                    .invalid(_, _, _, true) where folding:
                    continue
                default:
                    folding = false
                    pending.append(item)
                }
            }
            var itemsByTab = self.itemsByTab
            itemsByTab[.pending] = pending
            return ImportPreviewFixture(store: store, itemsByTab: itemsByTab)
        }
    }

    /// The list items and summaries the import previews are built from. Core
    /// computes the stable keys in production; these mirror the same shapes so
    /// a canvas addresses its rows the way the app does.
    extension PreviewData {
        static func candidateItem(
            _ row: BridgeTriageRow
        ) -> BridgeImportListItem {
            candidateItem(row, isGroupMember: false)
        }

        static func candidateItem(
            _ row: BridgeTriageRow,
            isGroupMember: Bool
        ) -> BridgeImportListItem {
            .candidate(
                stableKey: "candidate:\(row.candidateKey)",
                row: row,
                isGroupMember: isGroupMember
            )
        }

        static func invalidItem(
            _ candidate: BridgeInvalidCandidate
        ) -> BridgeImportListItem {
            .invalid(
                stableKey: "invalid:\(candidate.folderPath)",
                invalidCandidate: candidate,
                selected: false,
                isGroupMember: false
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
                    "group:\(key.watchedFolderPath.count)"
                    + key.watchedFolderPath + key.relativeFolderPath,
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

        static func importQueueSummary(
            pending: UInt32,
            done: UInt32,
            skipped: UInt32,
            watchedFolders: [BridgeWatchedFolder],
            hasGroups: Bool = false,
            narrowed: BridgeNarrowedCount? = nil,
            filterText: String = ""
        ) -> BridgeImportQueueSummary {
            BridgeImportQueueSummary(
                counts: BridgeTriageTabCounts(
                    pending: pending,
                    done: done,
                    skipped: skipped
                ),
                watchedFolders: watchedFolders,
                hasGroups: hasGroups,
                narrowed: narrowed,
                narrowing: BridgeImportListNarrowing(
                    tab: .pending,
                    filterText: filterText,
                    pendingFilter: .all
                ),
                firstSelectedPosition: nil
            )
        }

        /// A selection of `keys` as core would summarize it: what each
        /// candidate in `store` offers now, joined into the selection's offers.
        @MainActor
        static func importSelection(
            of keys: [String],
            in store: ImportStore
        ) -> ImportSelection {
            let members = keys.map { key in
                BridgeSelectionMember(
                    candidateKey: key,
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
    }
#endif
