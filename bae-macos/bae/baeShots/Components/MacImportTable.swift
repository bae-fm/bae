import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macImportTableScenes: [ShotScene] =
        [
            component(
                "mac.component.import-table.default",
                page: true,
                width: 968,
                height: 130
            ) {
                importTable([
                    BridgeTrackTableSide(
                        side: .disc(disc: 1),
                        runs: [.track(row: harborLights)]
                    )
                ])
            }
        ]

    private static func importTable(
        _ sides: [BridgeTrackTableSide]
    ) -> some View {
        importStores(
            MacImportTable(
                sides: sides,
                hasAlbumArtists: true,
                previewingTarget: nil,
                actions: inertMappingActions().trackTable,
                editingCommands: EditingCommitCommands()
            )
            .frame(width: 968, alignment: .leading)
        )
    }

    private static let harborLights = mappingTrack(
        file: "01 Harbor Lights.flac",
        position: "1",
        title: "Harbor Lights",
        seconds: 272
    )
}
