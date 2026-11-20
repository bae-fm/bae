import BaeKit
import SwiftUI

@testable import bae

// The import window's list column and the pane that several selected
// folders open.

extension ShotScene {
    static let importListComponentScenes: [ShotScene] =
        importRowScenes + importTabsScenes + bulkActionRowScenes
        + importBulkScenes

    // MARK: Rows

    /// A Found row of the import list, as the boards draw it.
    private static func listRow(
        _ folder: PreviewData.BoardFolder,
        title: String? = nil,
        artist: String? = nil,
        record: String? = nil,
        live: BridgeCandidateLiveState
    ) -> BridgeTriageRow {
        PreviewData.boardRow(
            folder,
            summary: title.map {
                BridgeTriageMetadataSummary(
                    albumTitle: $0,
                    albumArtists: .named(artist ?? "")
                )
            },
            record: record,
            live: live
        )
    }

    private static func lowTideRow() -> BridgeTriageRow {
        listRow(
            .lowTideHours,
            title: "Low Tide Hours",
            artist: "Mara Quill",
            record: "component-low-tide-hours",
            live: PreviewData.needsYou(.matches(count: 3))
        )
    }

    private static func copperSkyRow() -> BridgeTriageRow {
        listRow(
            .copperSky,
            title: "Copper Sky",
            artist: "Odile Fen",
            live: PreviewData.needsYou(.notFound)
        )
    }

    private static func importRow(_ row: BridgeTriageRow) -> some View {
        MacImportRow(
            row: row,
            coverContent: row.cover.map(ImageContent.init(bridge:)),
            isGroupMember: false
        )
    }

    /// The import percents the importing row's bar follows.
    private static func rowPercents() -> LatestStream<UInt32> {
        .importPercents(
            holding: [PreviewData.BoardFolder.northbound.key: 60]
        )
    }

    private static func importRows() -> some View {
        VStack(alignment: .leading, spacing: 0) {
            importRow(lowTideRow())
            importRow(
                listRow(
                    .untitled07,
                    live: PreviewData.needsYou(.nothingToLookUp)
                )
            )
            importRow(
                listRow(
                    .northbound,
                    title: "Northbound",
                    artist: "Mara Quill",
                    record: "component-northbound",
                    live: PreviewData.northboundImportingLive
                )
            )
            importRow(
                listRow(
                    .smallHours,
                    title: "Small Hours",
                    artist: "Tomas Brell",
                    live: PreviewData.triageLive(
                        [.skip, .revealFolder],
                        identifying: .queued,
                        standing: .identifying
                    )
                )
            )
        }
        .environment(\.importPercents, rowPercents())
    }

    private static let importRowScenes: [ShotScene] = [
        component(
            "mac.component.import-row.default",
            width: 420,
            height: 260,
            inset: 0,
            images: PreviewData.artImageStore()
        ) {
            importStores(importRows())
                .frame(width: 420, alignment: .leading)
        }
    ]

    // MARK: Tabs

    private static func listSlot(
        _ importStore: ImportStore,
        _ uiStore: UiStore,
        counts: BridgeTriageTabCounts
    ) -> ImportListSlot {
        ImportListSlot.preview(
            importStore: importStore,
            uiStore: uiStore,
            items: [],
            tabCounts: counts
        )
    }

    private static let listCounts = BridgeTriageTabCounts(
        pending: 14,
        done: 31,
        skipped: 2
    )

    private static func importTabsScene(
        _ id: String,
        tab: BridgeTriageTab = .pending,
        counts: BridgeTriageTabCounts = listCounts
    ) -> ShotScene {
        component(
            id,
            width: 436,
            height: 60
        ) {
            importStores(
                MacImportTabs(
                    activeTab: .constant(tab),
                    listSlot: listSlot(ImportStore(), UiStore(), counts: counts)
                )
                .frame(width: 420, alignment: .leading)
            )
        }
    }

    private static let importTabsScenes: [ShotScene] = [
        importTabsScene("mac.component.import-tabs.found-open"),
        importTabsScene("mac.component.import-tabs.imported-open", tab: .done),
        importTabsScene(
            "mac.component.import-tabs.skipped-open",
            tab: .skipped
        ),
        importTabsScene(
            "mac.component.import-tabs.nothing-found",
            counts: BridgeTriageTabCounts(pending: 0, done: 31, skipped: 0)
        ),
    ]

    // MARK: Filter row

    /// What the filter row shows beyond its field: the text typed and the
    /// count it narrows to, the work that runs and whether a filter is on.
    private struct FilterRowState {
        var typed = ""
        var narrowed: BridgeNarrowedCount?
        var identifying = false
        var importing = false
        var scanning = false
        var filtered = false
    }

    /// The buttons at the end of the filter row, as the import list's
    /// sidebar puts them.
    @ViewBuilder
    // MARK: Sidebar

    // MARK: Bulk pane

    private static func bulkActionRowScene(
        _ id: String,
        action: BridgeCandidateAction,
        count: UInt64,
        destination: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 456,
            height: 60
        ) {
            importStores(
                MacBulkActionRow(
                    offer: ImportCandidateActionOffer(
                        BridgeSelectionOffer(
                            action: action,
                            count: count,
                            enabled: count > 0
                        ),
                        rowKey: nil
                    ),
                    importDestination: destination
                        ? .constant(.thisDevice) : nil,
                    action: {}
                )
                .disabled(count == 0)
                .frame(width: 440)
            )
        }
    }

    private static let bulkActionRowScenes: [ShotScene] = [
        bulkActionRowScene(
            "mac.component.bulk-action-row.applies",
            action: .identify,
            count: 5
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.strong",
            action: .import,
            count: 3
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.applies-to-none",
            action: .cancelIdentification,
            count: 0
        ),
        bulkActionRowScene(
            "mac.component.bulk-action-row.with-destination",
            action: .import,
            count: 3,
            destination: true
        ),
    ]

    /// What a selection of five folders offers, two of them ready to
    /// import: every action the card lists, with how many it applies to.
    private static let bulkOffered: [(BridgeCandidateAction, UInt64)] = [
        (.import, 2), (.cancelImport, 0), (.combine, 0), (.separate, 0),
        (.identify, 5), (.cancelIdentification, 0), (.retryIdentification, 1),
        (.resetToFileMetadata, 5), (.clearMetadata, 5), (.skip, 5),
        (.restore, 0), (.revealFolder, 5),
    ]

    private static let bulkSelection = BridgeSelectionSummary(
        count: 5,
        single: nil,
        offers: bulkOffered.map { action, count in
            BridgeSelectionOffer(
                action: action,
                count: count,
                enabled: count > 0
            )
        }
    )

    private static func importBulk(
        syncs: Bool = false
    ) -> some View {
        let selection = ImportSelection()
        selection.apply(bulkSelection)
        return importStores(
            MacImportBulk(
                importDestination: .constant(.cloud),
                onPerform: { _ in }
            )
            .environment(selection)
            .frame(width: 580, height: 600),
            syncs: syncs
        )
    }

    private static let importBulkScenes: [ShotScene] = [
        component(
            "mac.component.import-bulk.several",
            width: 596,
            height: 620
        ) {
            importBulk()
        },
        component(
            "mac.component.import-bulk.to-the-cloud",
            width: 596,
            height: 620
        ) {
            importBulk(syncs: true)
        },
    ]

}
