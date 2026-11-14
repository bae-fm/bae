import BaeKit
import Foundation

@testable import bae

/// The import boards' own data: the folders, names, files and records the
/// boards in `design/mocks` draw, built as the app reads them, so a shot of
/// an import board differs from it only where the app does. The library
/// boards' counterpart is `boardAlbums`.
extension PreviewData {
    // MARK: - The boards' folders

    /// Every folder the import boards list or open, with the path it sits at
    /// below the watched folder and the name the pane's folder line gives it.
    enum BoardFolder: CaseIterable {
        case fieldRecordings, fieldRecordingsDisc2, lowTideHours, copperSky
        case untitled07, smallHours, winterKitchen, northbound
        case saltAndCedar, paperLanterns
        case driftwood, glassHarbor, lateArrivals

        var path: String {
            switch self {
            case .fieldRecordings:
                "\(Self.fieldRecordingsGroup)/Disc 1"
            case .fieldRecordingsDisc2:
                "\(Self.fieldRecordingsGroup)/Disc 2"
            case .lowTideHours: "Mara Quill - Low Tide Hours"
            case .copperSky: "Odile Fen - Copper Sky"
            case .untitled07: "2024-rips/untitled 07"
            case .smallHours: "Tomas Brell - Small Hours"
            case .winterKitchen: "Odile Fen - Winter Kitchen"
            case .northbound: "Mara Quill - Northbound (1974)"
            case .saltAndCedar: "The Slow Orchards - Salt and Cedar"
            case .paperLanterns: "The Slow Orchards - Paper Lanterns"
            case .driftwood: "Mara Quill - Driftwood"
            case .glassHarbor: "Ines Varga Trio - Glass Harbor"
            case .lateArrivals: "The Slow Orchards - Late Arrivals"
            }
        }

        /// The folder's name in the pane's folder line.
        var name: String {
            switch self {
            case .fieldRecordings: Self.fieldRecordingsGroup
            case .fieldRecordingsDisc2: "Disc 2"
            case .untitled07: "2024-rips/untitled 07"
            default: path
            }
        }

        static let fieldRecordingsGroup =
            "Mara Quill - Field Recordings (1979)"

        var key: Data {
            Data("\(PreviewData.importRootPath)/\(path)".utf8)
        }
    }

    static func boardFolder(
        _ folder: BoardFolder,
        files: BridgeCandidateFiles = boardFolderFiles(
            tracks: [],
            images: []
        )
    ) -> BridgeFolderCandidate {
        BridgeFolderCandidate(
            parts: [],
            candidateKey: folder.key,
            folderPath: folder.key,
            sourceFolderName: folder.name,
            watchedFolderPath: importWatchedFolder.path,
            files: files,
            trackCount: UInt32(files.files.filter { $0.role.isAudio }.count),
            skipped: false,
            isAdded: false
        )
    }

    // MARK: - Files and tracks

    /// One audio file of a board folder and the track it becomes.
    struct BoardTrack {
        let file: String
        let title: String
        let seconds: UInt64

        var durationMs: UInt64 { seconds * 1_000 }
    }

    /// Northbound's four tracks, one file each.
    static let northboundTracks = [
        BoardTrack(
            file: "01 Northbound.flac",
            title: "Northbound",
            seconds: 291
        ),
        BoardTrack(file: "02 Margins.flac", title: "Margins", seconds: 238),
        BoardTrack(file: "03 Salt Road.flac", title: "Salt Road", seconds: 320),
        BoardTrack(
            file: "04 Winter Light.flac",
            title: "Winter Light",
            seconds: 247
        ),
    ]

    /// The folder's own files: its audio, its images and a rip log.
    static func boardFolderFiles(
        tracks: [BoardTrack],
        images: [String]
    ) -> BridgeCandidateFiles {
        let audio = tracks.map {
            boardFile($0.file, size: 35_000_000, role: .audio)
        }
        let art = images.map { name in
            let file = previewArtFile(name)
            return boardFile(
                "\(name).jpg",
                size: 1_100_000,
                role: .artwork(
                    choice: BridgeCoverChoice(
                        selection: .releaseImage(fileId: "\(name).jpg"),
                        image: .local(file: file)
                    )
                )
            )
        }
        return BridgeCandidateFiles(
            fileMetadataIdentity: "board-\(tracks.count)-\(images.count)",
            files: audio + art + [
                boardFile("rip.log", size: 4_000, role: .document)
            ],
            coverFiles: art,
            sourceAudio: .uniform(
                descriptor: BridgeSourceAudioDescriptor(
                    layout: .file,
                    format: previewSourceAudioFormat
                )
            )
        )
    }

    static func boardFile(
        _ name: String,
        size: UInt64,
        role: BridgeFileRole
    ) -> BridgeCandidateFile {
        BridgeCandidateFile(
            file: BridgeFileInfo(
                name: name,
                size: size,
                dirPrefix: nil,
                fileName: name,
                audioFormat: role.isAudio ? previewSourceAudioFormat : nil
            ),
            role: role
        )
    }

    static func boardPath(_ name: String) -> Data {
        URL(fileURLWithPath: "/tmp/board/\(name)").pathBytes
    }

    static func boardMappingFile(
        _ name: String,
        size: UInt64,
        role: BridgeMappingRole,
        durationMs: UInt64? = nil
    ) -> BridgeMappingFile {
        BridgeMappingFile(
            fileId: name,
            name: name,
            size: size,
            previewTarget: role == .audio
                ? BridgePreviewTarget(
                    path: boardPath(name),
                    name: name,
                    startSample: 0,
                    endSample: nil
                ) : nil,
            durationMs: durationMs,
            audioFormat: role == .audio ? previewSourceAudioFormat : nil,
            role: role
        )
    }

    static func boardImage(_ name: String) -> BridgeMappingImage {
        BridgeMappingImage(
            fileId: "\(name).jpg",
            name: "\(name).jpg",
            file: previewArtFile(name)
        )
    }

    static func boardCover(_ name: String) -> BridgeCoverChoice {
        BridgeCoverChoice(
            selection: .releaseImage(fileId: "\(name).jpg"),
            image: .local(file: previewArtFile(name))
        )
    }

    static let boardArtists = [
        "Mara Quill": "artist-mara-quill"
    ]

    static func boardArtist(_ name: String) -> BridgeCredited {
        pickedArtist(name, artistId: boardArtists[name] ?? "artist-\(name)")
    }

    /// One track row of a board draft, bound to the file it plays from.
    struct BoardTrackRow {
        let id: String
        let title: String
        let fileId: String
    }

    /// A draft of `title` by `artist` with `tracks`, each bound to its file.
    static func boardEdit(
        title: String,
        artist: String,
        year: String,
        pressing: BridgeRawPressingEdit? = nil,
        tracks: [BoardTrackRow]
    ) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: title,
            albumArtistAssignments: [boardArtist(artist)],
            albumYear: year,
            pressing: pressing
                ?? BridgeRawPressingEdit(
                    year: year,
                    labels: [],
                    barcodes: [],
                    facts: pressingFacts()
                ),
            tracks: tracks.map {
                BridgeRawTrackEdit(
                    id: $0.id,
                    title: $0.title,
                    artistAssignments: .albumArtists,
                    file: .standalone(fileId: $0.fileId)
                )
            }
        )
    }

    static func northboundEdit() -> BridgeRawReleaseEdit {
        boardEdit(
            title: "Northbound",
            artist: "Mara Quill",
            year: "1974",
            tracks: northboundTracks.enumerated()
                .map {
                    BoardTrackRow(
                        id: "northbound-\($0.offset)",
                        title: $0.element.title,
                        fileId: $0.element.file
                    )
                }
        )
    }

    /// Northbound's four files, each the track it becomes, with the folder's
    /// two images and the files carried alongside.
    static func northboundMapping() -> BridgeMappingTable {
        let edit = northboundEdit()
        return BridgeMappingTable(
            images: [boardImage("Northbound"), boardImage("Northbound back")],
            trackSides: [
                BridgeMappingTrackSide(
                    side: .flat,
                    headerKey: nil,
                    runs: [
                        .tracks(
                            mappings: northboundTracks.enumerated()
                                .map {
                                    index,
                                    track in
                                    BridgeTrackMapping(
                                        source: .file(
                                            file: boardMappingFile(
                                                track.file,
                                                size: 35_000_000,
                                                role: .audio,
                                                durationMs: track.durationMs
                                            )
                                        ),
                                        track: edit.tracks[index],
                                        position: "\(index + 1)",
                                        durationMs: track.durationMs,
                                        lengthsDisagree: false
                                    )
                                }
                        )
                    ]
                )
            ],
            files: carriedBoardFiles,
            untitledTrackCount: 0
        )
    }

    static let carriedBoardFiles: [BridgeMappingFileRow] = [
        .file(file: boardMappingFile("rip.log", size: 4_000, role: .document)),
        .file(
            file: boardMappingFile("cover.jpg", size: 1_100_000, role: .image)
        ),
    ]

    // MARK: - Records

    static func boardRecords(_ key: String) -> [BridgeReleaseRecord] {
        [
            BridgeReleaseRecord(
                catalog: .musicBrainz,
                url: "https://musicbrainz.org/release/\(key)"
            )
        ]
    }

    static func boardLink(_ key: String) -> BridgeReleaseLink {
        .pressing(
            link: BridgePressingLink(
                record: BridgeLookupRef(catalog: .musicBrainz, key: key),
                partners: []
            )
        )
    }
}
