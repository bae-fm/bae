import BaeKit
import Foundation

@testable import bae

/// The folders the import boards open in the pane.
extension PreviewData {
    // MARK: - The panes' candidates

    /// A catalog's search that found nothing for the folder, as the run
    /// reads with only the sources `providers` asked.
    static func boardNothingFoundRun(
        album: String,
        artist: String,
        catalogNumber: String
    ) -> BridgeIdentifyRun {
        BridgeIdentifyRun(
            providers: [.musicBrainz],
            discId: .absent,
            barcode: .absent,
            catalog: .numbers(
                rows: [
                    BridgeSignalValueRow(
                        value: catalogNumber,
                        excluded: false,
                        cells: [
                            BridgeProviderCell(
                                source: .musicBrainz,
                                lookup: .noMatch
                            )
                        ]
                    )
                ],
                candidates: []
            ),
            isrc: .absent,
            search: .searched(
                album: album,
                artist: artist,
                cells: [
                    BridgeProviderCell(source: .musicBrainz, lookup: .noMatch)
                ]
            ),
            readingArtwork: false
        )
    }

    /// A draft with no titles over `files`, each file the audio of a blank
    /// track.
    static func boardBlankEdit(files: [String]) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "",
            albumArtistAssignments: [],
            albumYear: "",
            pressing: BridgeRawPressingEdit(
                year: "",
                labels: [],
                barcodes: [],
                facts: pressingFacts()
            ),
            tracks: files.enumerated()
                .map {
                    BridgeRawTrackEdit(
                        id: "blank-\($0.offset)",
                        title: "",
                        artistAssignments: .albumArtists,
                        file: .standalone(fileId: $0.element)
                    )
                }
        )
    }

    static func boardBlankMapping(files: [BoardTrack])
        -> BridgeMappingTable
    {
        let edit = boardBlankEdit(files: files.map(\.file))
        return BridgeMappingTable(
            images: [],
            trackSides: [
                BridgeMappingTrackSide(
                    side: .flat,
                    headerKey: nil,
                    runs: [
                        .tracks(
                            mappings: files.enumerated()
                                .map { index, file in
                                    BridgeTrackMapping(
                                        source: .file(
                                            file: boardMappingFile(
                                                file.file,
                                                size: 35_000_000,
                                                role: .audio,
                                                durationMs: file.durationMs
                                            )
                                        ),
                                        track: edit.tracks[index],
                                        position: "\(index + 1)",
                                        durationMs: file.durationMs,
                                        lengthsDisagree: false
                                    )
                                }
                        )
                    ]
                )
            ],
            files: [],
            untitledTrackCount: UInt32(files.count)
        )
    }

    /// The folders without a draft, which Find online is open on.
    static let unmatchedTracks = [
        BoardTrack(file: "01 Side A.flac", title: "", seconds: 1_284),
        BoardTrack(file: "02 Side B.flac", title: "", seconds: 1_342),
    ]

    /// A folder every lookup found nothing for, and what its pane shows of
    /// that.
    struct NotFoundFolder {
        let folder: BoardFolder
        let album: String
        let artist: String
        let catalogNumber: String
    }

    /// `found`'s folder opened on Find online's `section`, with the words
    /// typed in its search form when `typed`.
    @MainActor
    static func notFoundCandidate(
        _ found: NotFoundFolder,
        section: BridgeFindOnlineSection,
        typed: Bool
    ) -> Candidate {
        var candidate = paneCandidate(
            folder: boardFolder(
                found.folder,
                files: boardFolderFiles(tracks: unmatchedTracks, images: [])
            ),
            edit: boardBlankEdit(files: unmatchedTracks.map(\.file)),
            mapping: boardBlankMapping(files: unmatchedTracks),
            presentation: .findOnline,
            session: paneSession(
                presentation: .findOnline,
                section: section,
                artist: typed ? found.artist : "",
                album: typed ? found.album : ""
            ),
            resumedIdentifyState: .notFoundAnywhere(
                run: boardNothingFoundRun(
                    album: found.album,
                    artist: found.artist,
                    catalogNumber: found.catalogNumber
                )
            )
        )
        candidate.placement = .pending(folderCheck: nil, records: [])
        candidate.live = needsYou(.notFound)
        return candidate
    }

    /// Copper Sky's typed search, as the sources answer it: MusicBrainz has
    /// its album, Discogs is still out.
    static let boardSearchRun: BridgeCandidateSearch = {
        func pressing(
            _ year: Int32,
            _ catalog: String,
            _ id: String,
            _ facts: BridgePressingFacts
        ) -> BridgePressing {
            BridgePressing(
                releases: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: id,
                        year: year,
                        facts: facts,
                        barcodes: [],
                        sourceGroupId: "board-copper-sky"
                    )
                ],
                sources: [.musicBrainz],
                labels: [
                    BridgeLabelLine(
                        names: ["Fen Records"],
                        catalogNumbers: [catalog]
                    )
                ],
                pick: BridgePressingLink(
                    record: BridgeLookupRef(catalog: .musicBrainz, key: id),
                    partners: []
                )
            )
        }
        let group = BridgeReleaseGroup(
            id: "board-copper-sky",
            titles: ["Copper Sky"],
            artists: [.named("Odile Fen")],
            labels: ["Fen Records"],
            coverArt: nil,
            sources: [
                BridgeReleaseGroupSource(
                    source: .musicBrainz,
                    groupUrl:
                        "https://musicbrainz.org/release-group/board-copper-sky",
                    albumLinksUnread: false
                )
            ],
            yearMin: 1972,
            yearMax: 1995,
            sections: [
                BridgePressingSection(
                    album: nil,
                    pressings: [
                        pressing(
                            1972,
                            "FR-12",
                            "board-copper-sky-lp",
                            pressingFacts(country: "GB", media: media(.vinyl))
                        ),
                        pressing(
                            1995,
                            "FRCD-12",
                            "board-copper-sky-cd",
                            pressingFacts(
                                media: media(.cd),
                                discogsDetails: [.remastered]
                            )
                        ),
                    ],
                    narrowedOut: []
                )
            ]
        )
        return BridgeCandidateSearch(
            query: .general(artist: "Odile Fen", album: "Copper Sky"),
            unusableBarcode: nil,
            sources: searchSources(
                musicbrainz: .done(count: 1),
                discogs: .searching
            ),
            groups: [group],
            status: .searching
        )
    }()

    /// What Copper Sky has in flight: the typed search.
    static func boardSearchRuntime() -> [Data: BridgeCandidateRuntimeSnapshot] {
        [
            BoardFolder.copperSky.key: BridgeCandidateRuntimeSnapshot(
                identification: nil,
                import: nil,
                search: boardSearchRun,
                signals: nil
            )
        ]
    }

    /// Northbound as identified: the release's draft over four files.
    @MainActor
    static func northboundDraftCandidate() -> Candidate {
        var candidate = paneCandidate(
            folder: boardFolder(
                .northbound,
                files: boardFolderFiles(
                    tracks: northboundTracks,
                    images: ["Northbound", "Northbound back"]
                )
            ),
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: .musicBrainz,
                    key: "board-northbound"
                )
            ),
            releaseLink: boardLink("board-northbound"),
            edit: northboundEdit(),
            mapping: northboundMapping(),
            cover: boardCover("Northbound")
        )
        candidate.placement = .pending(
            folderCheck: nil,
            records: boardRecords("board-northbound")
        )
        candidate.live = triageLive(boardDraftActions)
        return candidate
    }

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

    /// A CUE sheet of Field Recordings' folder, on `disc`, asking for the
    /// audio file `reference`, which `boundTo` names when the folder has it.
    static func fieldRecordingsSheet(
        _ number: Int,
        bound: BridgeSheetBound,
        boundTo: String?
    ) -> BridgeSheetGroup {
        let name = "Field Recordings CD\(number).cue"
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
                            durationMs: entry.seconds * 1_000,
                            containerId: container.fileId,
                            containerName: container.name,
                            previewTarget: BridgePreviewTarget(
                                path: boardPath(container.name),
                                name: container.name,
                                startSample: from,
                                endSample: startSample
                            ),
                            audioFormat: previewSourceAudioFormat
                        )
                    ),
                    track: edit.tracks[index],
                    position: "\(index + 1)",
                    durationMs: entry.listedSeconds * 1_000,
                    lengthsDisagree: entry.seconds != entry.listedSeconds
                )
            }
    }

    /// Field Recordings' draft: two CDs from Harbor Records, three tracks,
    /// each carved from the first CD's audio by its sheet.
    static func fieldRecordingsEdit() -> BridgeRawReleaseEdit {
        return BridgeRawReleaseEdit(
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
            bound: .describes(container: fieldRecordingsContainer),
            boundTo: fieldRecordingsContainer.fileId
        )
        let sheetTwo = fieldRecordingsSheet(
            2,
            bound: .unresolved(requested: ["Field Recordings CD2.flac"]),
            boundTo: nil
        )
        let edit = fieldRecordingsEdit()
        var candidate = paneCandidate(
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
            cover: boardCover("Field Recordings")
        )
        candidate.placement = .pending(
            folderCheck: .sourceTracksUnknown,
            records: boardRecords("board-field-recordings")
        )
        candidate.live = triageLive(boardDraftActions)
        return candidate
    }

    /// Northbound after its import: the release this device made and the
    /// images its scan read.
    @MainActor
    static func northboundImportedCandidate() -> Candidate {
        let release = boardImported(.northbound)
        return Candidate(
            detail: BridgeImportCandidateDetail(
                candidate: boardFolder(
                    .northbound,
                    files: boardFolderFiles(
                        tracks: northboundTracks,
                        images: ["Northbound", "Northbound back"]
                    )
                ),
                actionable: true,
                resumedIdentifyState: .idle,
                placement: .done,
                live: triageLive([.revealFolder]),
                importStatus: .complete(release: release),
                signals: nil,
                stage: .imported(
                    release: release,
                    images: [
                        boardImage("Northbound"),
                        boardImage("Northbound back"),
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
            display: BridgeReleaseEditDisplayContext(
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
                            .map {
                                index,
                                track in
                                .track(
                                    track: BridgeReleaseEditTrackContext(
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
                                    )
                                )
                            }
                    )
                ],
                files: [
                    boardMappingFile("rip.log", size: 4_000, role: .document),
                    boardMappingFile(
                        "cover.jpg",
                        size: 1_100_000,
                        role: .image
                    ),
                ]
            ),
            album: .named(albumId: "board-album", title: edit.albumTitle),
            artistResolutions: [],
            changed: false,
            invalid: nil
        )
    }
}
