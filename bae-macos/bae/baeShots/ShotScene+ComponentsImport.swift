import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the import: the folder line and a group's
    /// header, then the mapping table with its rows, images and files, the
    /// list's column, the search's cards and pressings, the draft card and
    /// its parts, the skipped folder's and the release editor's cards, and
    /// the cover gallery.
    static let importComponentScenes: [ShotScene] =
        importValueScenes + importMappingScenes + importListComponentScenes
        + importSearchScenes + importDraftScenes + importCardScenes
        + importCoverGalleryScenes
        + importSheetScenes + importAnswerScenes + importSummaryScenes
        + importSidebarComponentScenes

    private static let importValueScenes: [ShotScene] = [
        component("mac.component.import-folder-line.default", height: 50) {
            MacImportFolderLine(
                tab: .pending,
                folderName: "Mara Quill - Low Tide Hours (1992) [FLAC]",
                folderPaths: [],
                onNavigateToPlacement: {}
            )
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.import-group-header.default", height: 60) {
            MacImportGroupHeader(
                group: BridgeTriageGroup(
                    key: PreviewData.releaseQueueGroupKey,
                    name: "Mara Quill — Field Recordings (1979)",
                    combinable: false
                ),
                expanded: true,
                onSetExpanded: { _ in },
                onCombine: {}
            )
            .frame(width: 380, height: 40)
        },
    ]
}

// MARK: - The mapping table

extension ShotScene {
    // MARK: Mapping

    /// What the import views read from the environment, with the stand-ins
    /// the other import shots use.
    static func importStores<Content: View>(
        _ content: Content,
        syncs: Bool = false
    ) -> some View {
        content
            .environment(ImportStore())
            .importPreviewStores(syncs: syncs)
    }

    /// One track of the mapping table: the file it plays from, its title
    /// and the position core numbers it by. By default the album's artists
    /// are its artists, so its artist field is empty; `ownArtists` is the
    /// field core's read gives the track's own `artists`; `listedSeconds` is
    /// the length the source lists when it differs from the audio's.
    private static func mappingTrack(
        file: String,
        position: String,
        title: String,
        seconds: UInt64,
        artists: BridgeTrackArtistAssignments = .albumArtists,
        ownArtists: BridgeArtistField? = nil,
        listedSeconds: UInt64? = nil
    ) -> BridgeTrackTableRow {
        BridgeTrackTableRow(
            track: BridgeRawTrackEdit(
                id: "component-\(file)",
                title: title,
                artistAssignments: artists,
                file: .standalone(fileId: file)
            ),
            source: .file(name: file),
            audition: PreviewData.wholeFile(file),
            position: position,
            length: BridgeDurationClock.length(ms: seconds * 1_000),
            listedLength: listedSeconds.map {
                BridgeDurationClock.length(ms: $0 * 1_000)
            },
            artists: ownArtists
        )
    }

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

    /// The mapping table over `sides`, with no images or other files.
    private static func mappingTable(
        sides: [BridgeTrackTableSide]
    ) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            trackSides: sides,
            files: [],
            untitledTrackCount: 0
        )
    }

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

    private static func fileRow(
        _ name: String,
        size: UInt64,
        role: BridgeMappingRole,
        plays: Bool = false
    ) -> BridgeMappingFileRow {
        .file(
            file: BridgeMappingFile(
                fileId: name,
                name: name,
                size: size,
                previewTarget: plays ? previewTarget(name) : nil,
                role: role
            )
        )
    }

    private static func previewTarget(_ name: String) -> BridgePreviewTarget {
        BridgePreviewTarget(
            path: Data(name.utf8),
            name: name,
            startSample: 0,
            endSample: nil
        )
    }

    private static let importMappingScenes: [ShotScene] =
        [
            component(
                "mac.component.import-table.default",
                width: 596,
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
        ] + importTrackRowScenes

    /// `MacImportTrackRow` in the mapping table that holds it, one track on
    /// its own flat side, in the preview's 820pt column.
    private static let importTrackRowScenes: [ShotScene] = [
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
    ]

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
            width: 836,
            height: 80
        ) {
            importStores(
                MacImportTable(
                    table: mappingTable(sides: [
                        BridgeTrackTableSide(
                            side: .flat,
                            runs: [.track(row: track)]
                        )
                    ]),
                    previewingTarget: playing
                        ? track.audition.previewTarget : nil,
                    actions: inertMappingActions(),
                    editingCommands: EditingCommitCommands()
                )
                .frame(width: 820, alignment: .leading)
            )
        }
    }

    private static let importSheetScenes: [ShotScene] = [
        component(
            "mac.component.import-sheet-caption.default",
            width: 596,
            height: 110
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                ReleaseSheetCaption(
                    sheet: PreviewData.fieldRecordingsSheet(
                        1,
                        bound: .describes(
                            container: PreviewData.fieldRecordingsContainer
                        )
                    ),
                    actions: inertMappingActions().sheetCaption
                )
                ReleaseSheetCaption(
                    sheet: PreviewData.fieldRecordingsSheet(
                        2,
                        bound: .unresolved(
                            requested: ["Field Recordings CD2.flac"]
                        )
                    ),
                    actions: inertMappingActions().sheetCaption
                )
            }
            .frame(width: 580, alignment: .leading)
        },
        importImagesScene(
            "mac.component.import-images.several",
            names: ["cover", "back", "disc"]
        ),
        importImagesScene("mac.component.import-images.one", names: ["cover"]),
        releaseFilesScene(
            "mac.component.release-files.files",
            height: 120,
            rows: [
                fileRow("rip.log", size: 4_000, role: .document),
                fileRow("notes.txt", size: 2_000, role: .document),
            ]
        ),
        releaseFilesScene(
            "mac.component.release-files.skipped-folder",
            height: 200,
            rows: [
                fileRow(
                    "01 Northbound.flac",
                    size: 33_000_000,
                    role: .audio,
                    plays: true
                ),
                fileRow(
                    "02 Margins.flac",
                    size: 31_800_000,
                    role: .damagedAudio(damage: .unreadable)
                ),
                fileRow("rip.log", size: 4_000, role: .document),
                fileRow(".DS_Store", size: 6_000, role: .other),
            ]
        ),
        releaseFilesScene(
            "mac.component.release-files.one-file",
            height: 80,
            rows: [fileRow("rip.log", size: 4_000, role: .document)]
        ),
    ]

    /// `MacImportImages` over `names`, the first the cover and the others
    /// its back and disc, each drawn as the release's artwork.
    private static func importImagesScene(
        _ id: String,
        names: [String]
    ) -> ShotScene {
        let art = ["cover": "", "back": " back", "disc": " disc"]
        return component(
            id,
            width: 596,
            height: 190,
            images: PreviewData.artImageStore()
        ) {
            MacImportImages(
                images: names.map {
                    PreviewData.boardImage(
                        "\($0).jpg",
                        art: "Low Tide Hours\(art[$0] ?? "")"
                    )
                },
                onOpen: { _ in }
            )
            .frame(width: 580, alignment: .leading)
        }
    }

    /// `MacReleaseFiles` over `rows`, in the preview's 520pt column.
    private static func releaseFilesScene(
        _ id: String,
        height: CGFloat,
        rows: [BridgeMappingFileRow]
    ) -> ShotScene {
        component(
            id,
            width: 536,
            height: height
        ) {
            MacReleaseFiles(
                rows: rows,
                onOpenDocument: { _, _ in },
                playing: (nil, inertMappingActions())
            )
            .frame(width: 520, alignment: .leading)
        }
    }
}
