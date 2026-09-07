#if DEBUG
    import BaeKit
    import Foundation

    /// One import-tab preview: the store both halves of the tab read, and the
    /// list items each tab holds. A canvas asks for the slot its `UiStore`'s
    /// tab names, which is what a live list would have delivered.
    struct ImportPreviewFixture {
        let store: ImportStore
        /// Fake editor responses belong to the preview's source, not its store.
        let candidates: [String: Candidate]
        let itemsByTab: [BridgeTriageTab: [BridgeImportListItem]]

        @MainActor
        func slot(uiStore: UiStore) -> ImportListSlot {
            applySelection(uiStore.selectedFolderCandidates)
            uiStore.onFolderCandidateSelectionChanged = { applySelection($0) }
            return ImportListSlot.preview(
                importStore: store,
                uiStore: uiStore,
                items: itemsByTab[uiStore.importCandidateTab] ?? []
            )
        }

        @MainActor
        func applySelection(_ keys: Set<String>) {
            store.editorCandidate =
                keys.count == 1 ? keys.first.flatMap { candidates[$0] } : nil
            // These fixed rows already carry the fake core's action answers.
            let rows = itemsByTab.values.flatMap { $0 }
                .compactMap { item -> BridgeTriageRow? in
                    guard case .candidate(_, let row, _) = item,
                        keys.contains(row.candidateKey)
                    else { return nil }
                    return row
                }
                .sorted { $0.candidateKey < $1.candidateKey }
            let offers = [
                BridgeCandidateAction.importReady, .identify,
                .retryIdentification, .useFileMetadata, .clearMetadata, .skip,
                .restore,
            ]
            .compactMap { action -> BridgeImportCandidateActionOffer? in
                let targets = rows.filter { $0.actions.contains(action) }
                    .map {
                        BridgeImportCandidateActionTarget(
                            key: $0.candidateKey,
                            displayName: $0.folderName
                        )
                    }
                return targets.isEmpty
                    ? nil
                    : BridgeImportCandidateActionOffer(
                        action: action,
                        candidates: targets
                    )
            }
            store.selection = BridgeImportSelection(
                candidateKeys: rows.map(\.candidateKey),
                offers: offers,
                canCombine: false
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

        /// The Ready set for a fixture: the rows a bulk import would act on,
        /// in the order the list holds them.
        static func readyRows(
            _ rows: [BridgeTriageRow]
        ) -> [BridgeReadyRowRef] {
            rows.filter(\.selectable)
                .map { row in
                    BridgeReadyRowRef(
                        candidateKey: row.candidateKey,
                        coverThumbnailUrl: row.matched?.coverThumbnailUrl
                    )
                }
        }

        static func importQueueSummary(
            pending: UInt32,
            done: UInt32,
            skipped: UInt32,
            watchedFolders: [BridgeWatchedFolder],
            folderScanStatuses: [BridgeWatchedFolderScanStatus] = [],
            folderScanActivity: BridgeFolderScanActivity? = nil,
            groupKeys: [BridgeFolderReleaseDecisionKey] = [],
            ready: [BridgeReadyRowRef] = [],
            firstUnidentified: BridgeFirstUnidentifiedRowRef? = nil
        ) -> BridgeImportQueueSummary {
            BridgeImportQueueSummary(
                counts: BridgeTriageTabCounts(
                    pending: pending,
                    done: done,
                    skipped: skipped
                ),
                watchedFolders: watchedFolders,
                folderScanStatuses: folderScanStatuses,
                folderScanActivity: folderScanActivity,
                groupKeys: groupKeys,
                ready: ready,
                firstUnidentified: firstUnidentified
            )
        }
    }
#endif
