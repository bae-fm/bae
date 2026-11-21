import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportTrackRow` in the mapping table that holds it, one track on
    /// its own flat side, in the preview's 820pt column.
    static let macImportTrackRowScenes: [ShotScene] = [
        trackRowScene(
            "mac.component.import-track-row.track",
            saltRoad
        ),
        trackRowScene(
            "mac.component.import-track-row.playing",
            saltRoad,
            playing: true
        ),
        trackRowScene(
            "mac.component.import-track-row.own-artist-from-the-library",
            mappingTrack(
                file: "03 Salt Road.flac",
                position: "3",
                title: "Salt Road",
                seconds: 320,
                artists: .explicit(
                    assignments: [PreviewData.boardArtist("Odile Fen")]
                ),
                ownArtists: PreviewData.boardLibraryArtists("Odile Fen")
            )
        ),
        trackRowScene(
            "mac.component.import-track-row.untitled",
            mappingTrack(
                file: "03 Salt Road.flac",
                position: "3",
                title: "",
                seconds: 320
            )
        ),
        trackRowScene(
            "mac.component.import-track-row.length-differs",
            mappingTrack(
                file: "03 Salt Road.flac",
                position: "3",
                title: "Salt Road",
                seconds: 320,
                listedSeconds: 327
            )
        ),
        trackRowScene(
            "mac.component.import-track-row.cue-track",
            cueTrack(
                position: "3",
                title: "Salt Road",
                startSeconds: 529,
                seconds: 320
            )
        ),
        trackRowScene(
            "mac.component.import-track-row.no-artist",
            mappingTrack(
                file: "03 Salt Road.flac",
                position: "3",
                title: "Salt Road",
                seconds: 320,
                artists: .explicit(assignments: [])
            )
        ),
    ]

    /// The track `seconds` long that a CUE sheet carves out of its image,
    /// starting `startSeconds` into it.
    private static func cueTrack(
        position: String,
        title: String,
        startSeconds: UInt64,
        seconds: UInt64
    ) -> BridgeTrackTableRow {
        let container = "Salt Road.flac"
        return BridgeTrackTableRow(
            track: BridgeRawTrackEdit(
                id: "component-cue-\(position)",
                title: title,
                artistAssignments: .albumArtists,
                file: .standalone(fileId: container)
            ),
            source: .sheetSlice(
                start: BridgeDurationClock.length(ms: startSeconds * 1_000)
            ),
            audition: .preview(
                target: BridgePreviewTarget(
                    path: PreviewData.fakePath(container),
                    name: container,
                    startSample: startSeconds * 44_100,
                    endSample: (startSeconds + seconds) * 44_100
                )
            ),
            position: position,
            length: BridgeDurationClock.length(ms: seconds * 1_000),
            listedLength: nil,
            artists: nil
        )
    }

    private static let saltRoad = mappingTrack(
        file: "03 Salt Road.flac",
        position: "3",
        title: "Salt Road",
        seconds: 320
    )

    private static func trackRowScene(
        _ id: String,
        _ track: BridgeTrackTableRow,
        playing: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 820,
            height: 80
        ) {
            importStores(
                MacImportTable(
                    sides: [
                        BridgeTrackTableSide(
                            side: .flat,
                            runs: [.track(row: track)]
                        )
                    ],
                    previewingTarget: playing
                        ? track.audition.previewTarget : nil,
                    actions: inertMappingActions().trackTable,
                    editingCommands: EditingCommitCommands()
                )
                .frame(width: 820, alignment: .leading)
            )
        }
    }
}
