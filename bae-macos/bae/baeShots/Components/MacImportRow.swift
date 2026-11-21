import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macImportRowScenes: [ShotScene] = [
        component(
            "mac.component.import-row.default",
            width: 420,
            height: 260,
            images: PreviewData.artImageStore()
        ) {
            importStores(importRows())
                .frame(width: 420, alignment: .leading)
        }
    ]

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
}
