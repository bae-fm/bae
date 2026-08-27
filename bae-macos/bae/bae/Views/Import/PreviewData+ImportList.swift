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
                        claim: row.claim
                            ?? .release(
                                releaseId: row.matched?.releaseId
                                    ?? row.candidateKey,
                                source: .musicBrainz
                            ),
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
                groupKeys: groupKeys,
                ready: ready,
                firstUnidentified: firstUnidentified
            )
        }
    }
#endif
