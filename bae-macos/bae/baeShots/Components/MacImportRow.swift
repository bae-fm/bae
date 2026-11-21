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
        },
        importRowScene("mac.component.import-row.badge-tone-danger") {
            importRow(
                lowTideRow(
                    live: PreviewData.triageLive(
                        [.skip, .revealFolder],
                        standing: .needsYou(reason: .matches(count: 3)),
                        badge: BridgeRowBadge(
                            says: .needsYou(reason: .matches(count: 3)),
                            tone: .failure
                        )
                    )
                )
            )
        },
        importRowScene("mac.component.import-row.grouped") {
            MacImportRow(
                row: lowTideRow(),
                coverContent: lowTideRow().cover
                    .map(
                        ImageContent.init(bridge:)
                    ),
                isGroupMember: true
            )
        },
        importRowScene("mac.component.import-row.selected") {
            importRow(lowTideRow())
                .background(Theme.accentSoft)
        },
        importRowScene("mac.component.import-row.identifying") {
            importRow(
                lowTideRow(
                    live: PreviewData.triageLive(
                        [.skip, .revealFolder],
                        identifying: .running,
                        standing: .identifying
                    )
                )
            )
        },
        importRowScene("mac.component.import-row.import-queued") {
            importRow(
                lowTideRow(
                    live: PreviewData.triageLive(
                        [],
                        importStep: .preparing(step: .queued)
                    )
                )
            )
        },
        importRowScene("mac.component.import-row.uploading") {
            importRow(uploadingRow())
                .environment(
                    uploadingOutbox(
                        PreviewData.boardImported(.lowTideHours).releaseId,
                        fraction: 0.3
                    )
                )
        },
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

    private static func lowTideRow(
        live: BridgeCandidateLiveState = PreviewData.needsYou(
            .matches(count: 3)
        )
    ) -> BridgeTriageRow {
        listRow(
            .lowTideHours,
            title: "Low Tide Hours",
            artist: "Mara Quill",
            record: "component-low-tide-hours",
            live: live
        )
    }

    /// Low Tide Hours after its import, while its files upload.
    private static func uploadingRow() -> BridgeTriageRow {
        PreviewData.boardRow(
            .lowTideHours,
            summary: BridgeTriageMetadataSummary(
                albumTitle: "Low Tide Hours",
                albumArtists: .named("Mara Quill")
            ),
            record: "component-low-tide-hours",
            placement: .done,
            live: PreviewData.triageLive([.revealFolder]),
            importStatus: .complete(
                release: PreviewData.boardImported(.lowTideHours)
            ),
            upload: BridgeUploadProgress(
                queued: 0,
                uploading: 1,
                activity: .uploading
            )
        )
    }

    private static func northboundRow() -> BridgeTriageRow {
        listRow(
            .northbound,
            title: "Northbound",
            artist: "Mara Quill",
            record: "component-northbound",
            live: PreviewData.northboundImportingLive
        )
    }

    /// One row of the import list in the preview's 420pt column.
    private static func importRowScene(
        _ id: String,
        @ViewBuilder row: @escaping @MainActor () -> some View
    ) -> ShotScene {
        component(
            id,
            width: 420,
            height: 70,
            images: PreviewData.artImageStore()
        ) {
            importStores(
                row()
                    .environment(\.importPercents, rowPercents())
                    .frame(width: 420, alignment: .leading)
            )
        }
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
