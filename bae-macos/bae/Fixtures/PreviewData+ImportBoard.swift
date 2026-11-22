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
    enum BoardFolder {
        case fieldRecordings, fieldRecordingsDisc2, lowTideHours, copperSky
        case untitled07, smallHours, winterKitchen, northbound
        case saltAndCedar, paperLanterns
        case driftwood, glassHarbor, lateArrivals
        case emptyFolder, harborWaltz

        var path: String {
            switch self {
            case .fieldRecordings:
                "\(Self.fieldRecordingsGroup)/Disc 1"
            case .fieldRecordingsDisc2:
                "\(Self.fieldRecordingsGroup)/Disc 2"
            case .lowTideHours: "Mara Quill - Low Tide Hours (1992) [FLAC]"
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
            case .emptyFolder: "old/empty folder"
            case .harborWaltz: "Ada Lindqvist - Harbor Waltz"
            }
        }

        /// The folder's name in the pane's folder line.
        var name: String {
            switch self {
            case .fieldRecordings: Self.fieldRecordingsGroup
            case .fieldRecordingsDisc2: "Disc 2"
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
            watchedFolderPath: importWatchedFolderPath,
            files: files,
            trackCount: UInt32(files.files.filter { $0.role.isAudio }.count),
            skipped: false
        )
    }

    /// `folder` as a candidate the list addresses by its key.
    static func boardCandidate(_ folder: BoardFolder) -> Candidate {
        Candidate(
            folder: boardFolder(folder),
            folders: .one(folderPath: folder.key)
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

    /// The folder's own files: its audio, its images and, when `log`, a rip
    /// log.
    static func boardFolderFiles(
        tracks: [BoardTrack],
        images: [BridgeMappingImage],
        log: Bool = true
    ) -> BridgeCandidateFiles {
        let audio = tracks.map {
            previewFile(name: $0.file, size: 35_000_000, role: .audio)
        }
        let art = images.map {
            previewFile(name: $0.name, size: $0.file.size, role: .artwork)
        }
        let documents =
            log
            ? [previewFile(name: "rip.log", size: 4_000, role: .document)]
            : []
        return BridgeCandidateFiles(
            fileMetadataIdentity: "board-\(tracks.count)-\(images.count)",
            files: audio + art + documents,
            coverFiles: art,
            sourceAudio: tracks.isEmpty
                ? nil
                : .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: previewSourceAudioFormat
                    )
                )
        )
    }

    /// The folder's image file `name`, drawn as the artwork `art`.
    static func boardImage(_ name: String, art: String) throws
        -> BridgeMappingImage
    {
        BridgeMappingImage(
            fileId: name,
            name: name,
            file: try BoardArtwork.scannedFile(named: art)
        )
    }

    struct NorthboundImages {
        let cover: BridgeMappingImage
        let back: BridgeMappingImage
        var gallery: [BridgeMappingImage] { [cover, back] }

    }

    struct LookupBoardImages {
        let lowTideHours: BridgeMappingImage
        let copperSky: BridgeMappingImage
        let winterKitchen: BridgeMappingImage
        let smallHours: BridgeFileVersion

        init() throws {
            lowTideHours = try lowTideHoursCover()
            copperSky = try notFoundCover(PreviewData.copperSky)
            winterKitchen = try notFoundCover(PreviewData.winterKitchen)
            smallHours = try BoardArtwork.scannedFile(named: "Small Hours")
        }
    }

    struct FoundBoardImages {
        let fieldRecordings: BridgeCoverChoice
        let lookup: LookupBoardImages
        let northbound: BridgeMappingImage
        let saltAndCedar: BridgeFileVersion
        let paperLanterns: BridgeFileVersion

        init() throws {
            fieldRecordings = try fieldRecordingsCatalogCover()
            lookup = try LookupBoardImages()
            northbound = try northboundCover()
            saltAndCedar = try BoardArtwork.scannedFile(named: "Salt and Cedar")
            paperLanterns = try BoardArtwork.scannedFile(
                named: "Paper Lanterns"
            )
        }
    }

    struct ImportedBoardRelease {
        let folder: BoardFolder
        let summary: BridgeTriageMetadataSummary
        let cover: BridgeFileVersion
        let upload: BridgeUploadProgress?
    }

    struct ImportedBoardContent {
        let northboundCover: BridgeMappingImage
        let releases: [ImportedBoardRelease]

        init() throws {
            northboundCover = try PreviewData.northboundCover()
            releases = [
                ImportedBoardRelease(
                    folder: .northbound,
                    summary: BridgeTriageMetadataSummary(
                        albumTitle: "Northbound",
                        albumArtists: .named("Mara Quill")
                    ),
                    cover: northboundCover.file,
                    upload: BridgeUploadProgress(
                        queued: 0,
                        uploading: 1,
                        activity: .uploading
                    )
                ),
                try Self.release(
                    .driftwood,
                    title: "Driftwood",
                    artist: "Mara Quill"
                ),
                try Self.release(
                    .glassHarbor,
                    title: "Glass Harbor",
                    artist: "Ines Varga Trio"
                ),
                try Self.release(
                    .lateArrivals,
                    title: "Late Arrivals",
                    artist: "The Slow Orchards"
                ),
            ]
        }

        private static func release(
            _ folder: BoardFolder,
            title: String,
            artist: String
        ) throws -> ImportedBoardRelease {
            ImportedBoardRelease(
                folder: folder,
                summary: BridgeTriageMetadataSummary(
                    albumTitle: title,
                    albumArtists: .named(artist)
                ),
                cover: try BoardArtwork.scannedFile(named: title),
                upload: nil
            )
        }
    }

    private static func northboundCover() throws -> BridgeMappingImage {
        try boardImage("cover.jpg", art: "Northbound")
    }

    static func northboundImages(cover: BridgeMappingImage) throws
        -> NorthboundImages
    {
        NorthboundImages(
            cover: cover,
            back: try boardImage("back.jpg", art: "Northbound back")
        )
    }

    static func boardCover(_ image: BridgeMappingImage) -> BridgeCoverChoice {
        BridgeCoverChoice(
            selection: .releaseImage(fileId: image.fileId),
            image: .local(file: image.file)
        )
    }

    // MARK: - Artists

    static func boardArtist(_ name: String) -> BridgeCredited {
        pickedArtist(name, artistId: "board-artist-\(name)")
    }

    /// A board draft's album artist `name`, picked from the library, as the
    /// pane's read answers the field: the artist is in the library.
    static func boardLibraryArtists(_ name: String) -> BridgeArtistField {
        BridgeArtistField(
            standing: .library,
            credits: [
                BridgeStandingCredit(
                    credited: boardArtist(name),
                    standing: .library
                )
            ]
        )
    }

    // MARK: - Drafts

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

    /// Northbound's draft: a CD from Harbor Records, pressed in the US.
    static func northboundEdit() -> BridgeRawReleaseEdit {
        boardEdit(
            title: "Northbound",
            artist: "Mara Quill",
            year: "1974",
            pressing: BridgeRawPressingEdit(
                year: "1974",
                labels: [
                    .alone(name: "Harbor Records", catalogNumber: "HR-0412")
                ],
                barcodes: [],
                facts: pressingFacts(
                    country: "US",
                    media: media(.cd),
                    status: .official
                )
            ),
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

    /// `tracks`, each file the track `edit` puts on it, in one flat side.
    static func boardTrackSides(
        _ tracks: [BoardTrack],
        edit: BridgeRawReleaseEdit
    ) -> [BridgeTrackTableSide] {
        [
            BridgeTrackTableSide(
                side: .flat,
                runs: tracks.enumerated()
                    .map { index, track in
                        .track(
                            row: tableRow(
                                edit.tracks[index],
                                source: .file(name: track.file),
                                audition: wholeFile(track.file),
                                position: "\(index + 1)",
                                lengthMs: track.durationMs
                            )
                        )
                    }
            )
        ]
    }

    /// Northbound's four files, each the track it becomes, with the folder's
    /// two images and the files carried alongside.
    static func northboundMapping(images: NorthboundImages)
        -> BridgeMappingTable
    {
        BridgeMappingTable(
            images: images.gallery,
            trackSides: boardTrackSides(
                northboundTracks,
                edit: northboundEdit()
            ),
            files: carriedBoardFiles.map { .file(file: $0) },
            untitledTrackCount: 0
        )
    }

    /// The files a board release carries that no track plays: its rip log
    /// beside the images the gallery shows.
    static let carriedBoardFiles: [BridgeMappingFile] = [
        mappingFile(
            previewFile(name: "rip.log", size: 4_000, role: .document),
            role: .document
        )
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
