import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macImportTableScenes: [ShotScene] =
        [
            component(
                "mac.component.import-table.default",
                page: true,
                width: 580,
                height: 130
            ) {
                importTable(
                    mappingTable(sides: [
                        BridgeTrackTableSide(
                            side: .disc(disc: 1),
                            runs: [.track(row: harborLights)]
                        )
                    ])
                )
            }
        ]

    private static func importTable(_ table: BridgeMappingTable) -> some View {
        importStores(
            MacImportTable(
                table: table,
                previewingTarget: nil,
                actions: inertMappingActions(),
                editingCommands: EditingCommitCommands()
            )
            .frame(width: 580, alignment: .leading)
        )
    }

    private static let harborLights = mappingTrack(
        file: "01 Harbor Lights.flac",
        position: "1",
        title: "Harbor Lights",
        seconds: 272
    )
}
