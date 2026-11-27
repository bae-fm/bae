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
                artist: .own(
                    field: PreviewData.boardLibraryArtists("Odile Fen")
                )
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
            "mac.component.import-track-row.added-by-the-save",
            unsaved(saltRoad)
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
                artists: .explicit(assignments: []),
                artist: .empty
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
                audition: BridgeFileAudition(
                    target: BridgePreviewTarget(
                        path: PreviewData.fakePath(container),
                        name: container,
                        startSample: startSeconds * 44_100,
                        endSample: (startSeconds + seconds) * 44_100
                    ),
                    previewing: false
                )
            ),
            position: position,
            length: BridgeDurationClock.length(ms: seconds * 1_000),
            listedLength: nil,
            artist: .albumArtist
        )
    }

    /// The track a save adds: nothing plays it yet.
    private static func unsaved(_ row: BridgeTrackTableRow)
        -> BridgeTrackTableRow
    {
        BridgeTrackTableRow(
            track: row.track,
            source: row.source,
            audition: .unsaved,
            position: row.position,
            length: row.length,
            listedLength: row.listedLength,
            artist: row.artist
        )
    }

    private static let saltRoad = mappingTrack(
        file: "03 Salt Road.flac",
        position: "3",
        title: "Salt Road",
        seconds: 320
    )

    /// `row` while the preview player holds its stretch of audio.
    private static func previewing(_ row: BridgeTrackTableRow)
        -> BridgeTrackTableRow
    {
        guard var audition = row.audition.fileAudition else {
            preconditionFailure("a folder's row previews its file")
        }
        audition.previewing = true
        var row = row
        row.audition = .preview(audition: audition)
        return row
    }

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
                            runs: [
                                .track(
                                    row: playing ? previewing(track) : track
                                )
                            ]
                        )
                    ],

                    actions: inertMappingActions().trackTable,
                    editingCommands: EditingCommitCommands()
                )
                .frame(width: 820, alignment: .leading)
            )
        }
    }
}
