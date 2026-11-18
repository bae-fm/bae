import BaeKit
import Foundation

@testable import bae

/// The folders the import boards open in the pane.
extension PreviewData {
    // MARK: - Northbound

    /// Northbound's folder: four files, two images and a rip log.
    static func northboundFolder() -> BridgeFolderCandidate {
        boardFolder(
            .northbound,
            files: boardFolderFiles(
                tracks: northboundTracks,
                images: ["Northbound", "Northbound back"]
            )
        )
    }

    /// Northbound as identified: the release's draft over four files, or,
    /// with `importStep`, that draft while its import runs at the step.
    @MainActor
    static func northboundDraftCandidate(
        importStep: BridgeImportStep? = nil
    ) -> Candidate {
        paneCandidate(
            folder: northboundFolder(),
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: .musicBrainz,
                    key: "board-northbound"
                )
            ),
            releaseLink: boardLink("board-northbound"),
            edit: northboundEdit(),
            mapping: northboundMapping(),
            cover: boardCover("Northbound"),
            placement: .pending(
                folderCheck: nil,
                records: boardRecords("board-northbound")
            ),
            live: importStep == nil
                ? triageLive(boardDraftActions) : northboundImportingLive,
            importStatus: importStep.map { .importing(step: $0) },
            albumArtistStanding: boardLibraryArtistStanding
        )
    }

    /// Northbound after its import: the release this device made and the
    /// images its scan read.
    @MainActor
    static func northboundImportedCandidate() -> Candidate {
        let release = boardImported(.northbound)
        return Candidate(
            detail: BridgeImportCandidateDetail(
                candidate: northboundFolder(),
                partCount: 0,
                actionable: true,
                placement: .done,
                live: triageLive([.revealFolder]),
                importStatus: .complete(release: release),
                signals: nil,
                stage: .imported(
                    release: release,
                    images: [
                        boardImage("Northbound.jpg"),
                        boardImage("Northbound back.jpg"),
                    ]
                )
            )
        )
    }

    /// What the library holds for the imported Northbound: its draft, each
    /// track beside the file it plays from, and the files no track plays.
    static func northboundReleaseDraft() -> BridgeReleaseEditDraft {
        let edit = northboundEdit()
        return BridgeReleaseEditDraft(
            edit: edit,
            cover: nil,
            sourceAudio: .uniform(
                descriptor: BridgeSourceAudioDescriptor(
                    layout: .file,
                    format: previewSourceAudioFormat
                )
            ),
            sides: [
                BridgeReleaseEditTrackSide(
                    side: .flat,
                    headerKey: nil,
                    runs: northboundTracks.enumerated()
                        .map { index, track in
                            .track(
                                row: BridgeReleaseEditTrackRow(
                                    edit: edit.tracks[index],
                                    context: BridgeReleaseEditTrackContext(
                                        trackId: edit.tracks[index].id,
                                        source: BridgeReleaseEditTrackSource(
                                            fileId: track.file,
                                            name: track.file,
                                            layout: .file
                                        ),
                                        durationClock: bridgeLengthClock(
                                            ms: track.durationMs
                                        ),
                                        positionText: "\(index + 1)"
                                    ),
                                    artistStanding: nil
                                )
                            )
                        }
                )
            ],
            files: carriedBoardFiles,
            album: .named(
                albumId: boardImported(.northbound).albumId,
                line: .titleAndArtists(
                    title: edit.albumTitle,
                    artistLine: .named("Mara Quill")
                )
            ),
            albumArtistStanding: boardLibraryArtistStanding,
            changed: false,
            invalid: nil
        )
    }

    // MARK: - Low Tide Hours

    /// Low Tide Hours' two files.
    static let lowTideHoursTracks = [
        BoardTrack(
            file: "01 Low Tide Hours.flac",
            title: "Low Tide Hours",
            seconds: 303
        ),
        BoardTrack(
            file: "02 Shoreline.flac",
            title: "Shoreline",
            seconds: 251
        ),
    ]

    /// Low Tide Hours' draft, read from MusicBrainz.
    static func lowTideHoursEdit() -> BridgeRawReleaseEdit {
        boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            tracks: lowTideHoursTracks.enumerated()
                .map {
                    BoardTrackRow(
                        id: "low-tide-\($0.offset)",
                        title: $0.element.title,
                        fileId: $0.element.file
                    )
                }
        )
    }

    /// Where the library holds Low Tide Hours: the one album, which holds
    /// the release itself.
    static let lowTideHoursInLibrary = BridgeLibraryStatus(
        releaseId: "board-low-tide-hours",
        releaseInLibrary: true,
        albumInLibrary: true,
        albums: [
            BridgeLibraryAlbumMatch(
                albumId: "board-0",
                albumTitle: "Low Tide Hours",
                holdsRelease: true
            )
        ]
    )

    /// Low Tide Hours' draft, a release the library already holds: its
    /// pick read from MusicBrainz and Discogs, its artist in the library.
    @MainActor
    static func lowTideHoursCandidate() -> Candidate {
        let tracks = lowTideHoursTracks
        let edit = lowTideHoursEdit()
        let records =
            boardRecords("board-low-tide-hours") + [
                BridgeReleaseRecord(
                    catalog: .discogs,
                    url: "https://www.discogs.com/release/board-low-tide-hours"
                )
            ]
        return paneCandidate(
            folder: boardFolder(
                .lowTideHours,
                files: boardFolderFiles(
                    tracks: tracks,
                    images: ["Low Tide Hours"]
                )
            ),
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: .musicBrainz,
                    key: "board-low-tide-hours"
                )
            ),
            releaseLink: boardLink("board-low-tide-hours"),
            edit: edit,
            mapping: BridgeMappingTable(
                images: [boardImage("Low Tide Hours.jpg")],
                trackSides: boardTrackSides(tracks, edit: edit),
                files: [],
                untitledTrackCount: 0
            ),
            cover: boardCover("Low Tide Hours"),
            placement: .pending(folderCheck: nil, records: records),
            live: needsYou(.matches(count: 3)),
            libraryStatus: lowTideHoursInLibrary,
            albumArtistStanding: boardLibraryArtistStanding
        )
    }

    // MARK: - untitled 07

    /// untitled 07's three files, none of them tagged.
    static let untitledTracks = [
        BoardTrack(file: "track01.flac", title: "", seconds: 252),
        BoardTrack(file: "track02.flac", title: "", seconds: 238),
        BoardTrack(file: "track03.flac", title: "", seconds: 320),
    ]

    /// untitled 07, from whose files bae read nothing to look it up by: a
    /// blank draft over its three files.
    @MainActor
    static func untitledCandidate() -> Candidate {
        let edit = blankDraft(files: untitledTracks.map(\.file))
        return paneCandidate(
            folder: boardFolder(
                .untitled07,
                files: boardFolderFiles(
                    tracks: untitledTracks,
                    images: [],
                    log: false
                )
            ),
            edit: edit,
            mapping: BridgeMappingTable(
                images: [],
                trackSides: boardTrackSides(untitledTracks, edit: edit),
                files: [],
                untitledTrackCount: UInt32(untitledTracks.count)
            ),
            live: needsYou(.nothingToLookUp)
        )
    }

    // MARK: - Field Recordings

    /// The audio the first of Field Recordings' sheets describes.
    static let fieldRecordingsContainer = BridgeMappingContainer(
        fileId: "Field Recordings CD1.flac",
        name: "Field Recordings CD1.flac",
        size: 340_000_000,
        audioFormat: previewSourceAudioFormat
    )

    /// One entry of a sheet: its title, the seconds its slice of the audio
    /// runs, and the seconds the release lists for it.
    struct CueEntry {
        let title: String
        let seconds: UInt64
        let listedSeconds: UInt64
    }

    static let fieldRecordingsEntries = [
        CueEntry(title: "Slow Morning", seconds: 622, listedSeconds: 622),
        CueEntry(
            title: "Weather Report Card",
            seconds: 276,
            listedSeconds: 284
        ),
        CueEntry(title: "Night Ferry", seconds: 276, listedSeconds: 276),
    ]

    /// A CUE sheet of Field Recordings' folder, on disc `number`, asking for
    /// its disc's audio file, bound as `bound` says.
    static func fieldRecordingsSheet(
        _ number: Int,
        bound: BridgeSheetBound
    ) -> BridgeSheetGroup {
        let name = "Field Recordings CD\(number).cue"
        let boundTo: String? =
            if case .describes(let container) = bound {
                container.fileId
            }
            else {
                nil
            }
        return BridgeSheetGroup(
            sheetId: name,
            name: name,
            size: 1_200,
            bound: bound,
            referenceOptions: [
                BridgeSheetReferenceOptions(
                    fileReference: "Field Recordings CD\(number).flac",
                    fileId: boundTo,
                    options: [
                        BridgeSheetBindingOption(
                            fileId: fieldRecordingsContainer.fileId,
                            offer: .offered
                        )
                    ]
                )
            ],
            assignment: .disc(number: UInt32(number)),
            discOptions: [1, 2]
        )
    }

    /// The first sheet's entries as mapping rows, each slice starting where
    /// the one before it ends.
    static func fieldRecordingsMappings(
        sheet: BridgeSheetGroup,
        edit: BridgeRawReleaseEdit
    ) -> [BridgeTrackMapping] {
        let container = fieldRecordingsContainer
        var startSample: UInt64 = 0
        return fieldRecordingsEntries.enumerated()
            .map { index, entry in
                let from = startSample
                startSample += entry.seconds * 44_100
                return BridgeTrackMapping(
                    source: .sheetEntry(
                        entry: BridgeMappingEntry(
                            sheetId: sheet.sheetId,
                            index: UInt32(index),
                            number: UInt32(index + 1),
                            title: entry.title,
                            containerId: container.fileId,
                            previewTarget: BridgePreviewTarget(
                                path: fakePath(container.name),
                                name: container.name,
                                startSample: from,
                                endSample: startSample
                            ),
                            audioFormat: previewSourceAudioFormat
                        )
                    ),
                    track: edit.tracks[index],
                    position: "\(index + 1)",
                    length: bridgeLengthClock(ms: entry.seconds * 1_000),
                    listedLength: entry.seconds == entry.listedSeconds
                        ? nil
                        : bridgeLengthClock(ms: entry.listedSeconds * 1_000),
                    artistStanding: nil
                )
            }
    }

    /// Field Recordings' draft: two CDs from Harbor Records, three tracks,
    /// each carved from the first CD's audio by its sheet.
    static func fieldRecordingsEdit() -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "Field Recordings",
            albumArtistAssignments: [boardArtist("Mara Quill")],
            albumYear: "1979",
            pressing: BridgeRawPressingEdit(
                year: "1979",
                labels: [
                    BridgeRawLabelEdit(
                        name: "Harbor Records",
                        catalogNumber: "HR-0779"
                    )
                ],
                barcodes: [],
                facts: pressingFacts(
                    country: "US",
                    media: media(.cd, 2),
                    status: .official,
                    packaging: .jewelCase
                )
            ),
            tracks: fieldRecordingsEntries.enumerated()
                .map { index, entry in
                    BridgeRawTrackEdit(
                        id: "fr-\(index)",
                        title: entry.title,
                        artistAssignments: .albumArtists,
                        file: .sheetSlice(
                            fileId: fieldRecordingsContainer.fileId,
                            sheetId: "Field Recordings CD1.cue",
                            index: UInt32(index)
                        )
                    )
                }
        )
    }

    /// Field Recordings: two CD sheets, the first bound to its audio and
    /// carving three tracks, the second waiting for the audio it names. The
    /// release lists no tracks.
    @MainActor
    static func fieldRecordingsCandidate() -> Candidate {
        let sheetOne = fieldRecordingsSheet(
            1,
            bound: .describes(container: fieldRecordingsContainer)
        )
        let sheetTwo = fieldRecordingsSheet(
            2,
            bound: .unresolved(requested: ["Field Recordings CD2.flac"])
        )
        let edit = fieldRecordingsEdit()
        return paneCandidate(
            folder: boardFolder(.fieldRecordings),
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: .musicBrainz,
                    key: "board-field-recordings"
                )
            ),
            releaseLink: boardLink("board-field-recordings"),
            edit: edit,
            mapping: BridgeMappingTable(
                images: [],
                trackSides: [
                    BridgeMappingTrackSide(
                        side: .flat,
                        headerKey: nil,
                        runs: [
                            .sheet(
                                sheet: sheetOne,
                                entries: fieldRecordingsMappings(
                                    sheet: sheetOne,
                                    edit: edit
                                )
                            ),
                            .sheet(sheet: sheetTwo, entries: []),
                        ]
                    )
                ],
                files: [],
                untitledTrackCount: 0
            ),
            cover: boardCover("Field Recordings"),
            placement: .pending(
                folderCheck: .sourceTracksUnknown,
                records: boardRecords("board-field-recordings")
            ),
            live: triageLive(boardDraftActions),
            albumArtistStanding: boardLibraryArtistStanding
        )
    }

    // MARK: - Skipped

    /// The empty folder as its read answers it: no images, no files.
    static let boardEmptyFolderDetail = BridgeInvalidCandidateDetail(
        candidate: boardEmptyFolder,
        images: [],
        files: []
    )
}
