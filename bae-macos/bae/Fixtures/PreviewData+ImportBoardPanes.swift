import BaeKit
import Foundation

@testable import bae

/// The folders the import boards open in the pane.
extension PreviewData {
    // MARK: - Northbound

    /// Northbound's folder: four files, two images and a rip log.
    static func northboundFolder(images: NorthboundImages)
        -> BridgeFolderCandidate
    {
        boardFolder(
            .northbound,
            files: boardFolderFiles(
                tracks: northboundTracks,
                images: images.gallery
            )
        )
    }

    /// Northbound as identified: the release's draft over four files, or,
    /// with `importStep`, that draft while its import runs at the step.
    @MainActor
    static func northboundDraftCandidate(
        cover: BridgeMappingImage,
        importStep: BridgeImportStep? = nil
    ) throws -> ImportPaneFixture {
        let images = try NorthboundImages(cover: cover)
        return paneFixture(
            folder: northboundFolder(images: images),
            metadataProvenance: .externalRelease(
                record: BridgeLookupRef(
                    catalog: .musicBrainz,
                    key: "board-northbound"
                )
            ),
            releaseLink: boardLink("board-northbound"),
            edit: northboundEdit(),
            releaseLines: releaseLines(of: northboundEdit(), minutes: 18),
            mapping: northboundMapping(images: images),
            cover: boardCover(images.cover),
            placement: .pending(
                folderCheck: nil,
                records: boardRecords("board-northbound")
            ),
            live: importStep == nil
                ? triageLive(boardDraftActions) : northboundImportingLive,
            importStatus: importStep.map { .importing(step: $0) },
            albumArtists: boardLibraryArtists("Mara Quill")
        )
    }

    /// Northbound after its import: the release this device made and the
    /// images its scan read.
    @MainActor
    static func northboundImportedCandidate(
        cover: BridgeMappingImage
    ) throws -> ImportPaneFixture {
        let images = try NorthboundImages(cover: cover)
        let release = boardImported(.northbound)
        return ImportPaneFixture(
            detail: BridgeImportCandidateDetail(
                candidate: northboundFolder(images: images),
                partCount: 0,
                actionable: true,
                placement: .done,
                live: triageLive([.revealFolder]),
                importStatus: .complete(release: release),
                signals: nil,
                stage: .imported(
                    release: release,
                    images: images.gallery
                )
            )
        )
    }

    /// What the library holds for the imported Northbound, as its editor
    /// opens on it: the form's values, and the draft with each track beside
    /// the file it plays from and the files no track plays.
    static func northboundRelease() -> PreviewReleaseEdit {
        let edit = northboundEdit()
        let draft = BridgeReleaseEditDraft(
            cover: nil,
            sourceAudio: .uniform(
                descriptor: BridgeSourceAudioDescriptor(
                    layout: .file,
                    format: previewSourceAudioFormat
                )
            ),
            sides: [
                BridgeTrackTableSide(
                    side: .flat,
                    runs: northboundTracks.enumerated()
                        .map { index, track in
                            .track(
                                row: tableRow(
                                    edit.tracks[index],
                                    source: .file(name: track.file),
                                    audition: .libraryTrack,
                                    position: "\(index + 1)",
                                    lengthMs: track.durationMs
                                )
                            )
                        }
                )
            ],
            files: carriedBoardFiles.map { .file(file: $0) },
            album: .named(
                albumId: boardImported(.northbound).albumId,
                line: .titleAndArtists(
                    title: edit.albumTitle,
                    artistLine: .named("Mara Quill")
                )
            ),
            records: boardRecords("board-northbound"),
            pressingOffers: pressingOffers,
            albumArtists: boardLibraryArtists("Mara Quill"),
            changed: false,
            filesToRead: nil,
            deletesTrackData: false,
            invalid: nil
        )
        return PreviewReleaseEdit(values: edit, draft: draft)
    }

    // MARK: - Low Tide Hours

    /// Low Tide Hours' five files.
    static let lowTideHoursTracks = [
        BoardTrack(
            file: "01 Harbor Lights.flac",
            title: "Harbor Lights",
            seconds: 272
        ),
        BoardTrack(
            file: "02 The Long Way Home.flac",
            title: "The Long Way Home",
            seconds: 312
        ),
        BoardTrack(
            file: "03 Second Floor.flac",
            title: "Second Floor",
            seconds: 225
        ),
        BoardTrack(
            file: "04 Low Tide Hours.flac",
            title: "Low Tide Hours",
            seconds: 303
        ),
        BoardTrack(
            file: "05 Tin Roof.flac",
            title: "Tin Roof",
            seconds: 341
        ),
    ]

    static func lowTideHoursCover() throws -> BridgeMappingImage {
        try boardImage("cover.jpg", art: "Low Tide Hours")
    }

    /// Low Tide Hours' draft, read from MusicBrainz.
    static func lowTideHoursEdit() -> BridgeRawReleaseEdit {
        boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: BridgeRawPressingEdit(
                year: "1992",
                labels: [
                    .alone(name: "Harbor Records", catalogNumber: "HR-1042")
                ],
                barcodes: [.alone("0 41234 56789 2")],
                facts: pressingFacts(
                    country: "US",
                    media: media(.cd),
                    status: .official
                )
            ),
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

    /// The files no track plays from: the rip log.
    static let lowTideHoursFiles = [
        mappingFile(
            previewFile(name: "rip.log", size: 4_000, role: .document),
            role: .document
        )
    ]

    /// Low Tide Hours' draft, a release the library already holds: its
    /// pick read from MusicBrainz and Discogs, its artist in the library.
    @MainActor
    static func lowTideHoursCandidate(cover: BridgeMappingImage)
        -> ImportPaneFixture
    {
        let tracks = lowTideHoursTracks
        let edit = lowTideHoursEdit()
        let records =
            boardRecords("board-low-tide-hours") + [
                BridgeReleaseRecord(
                    catalog: .discogs,
                    url: "https://www.discogs.com/release/board-low-tide-hours"
                )
            ]
        return paneFixture(
            folder: boardFolder(
                .lowTideHours,
                files: boardFolderFiles(
                    tracks: tracks,
                    images: [cover]
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
                images: [cover],
                trackSides: boardTrackSides(tracks, edit: edit),
                files: lowTideHoursFiles.map { .file(file: $0) },
                untitledTrackCount: 0
            ),
            cover: boardCover(cover),
            placement: .pending(folderCheck: nil, records: records),
            live: needsYou(.matches(count: 3)),
            libraryStatus: lowTideHoursInLibrary,
            album: .named(
                albumId: "board-0",
                line: .titleAndArtists(
                    title: "Low Tide Hours",
                    artistLine: .named("Mara Quill")
                )
            ),
            albumArtists: boardLibraryArtists("Mara Quill")
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
    static func untitledCandidate() -> ImportPaneFixture {
        let edit = blankDraft(files: untitledTracks.map(\.file))
        return paneFixture(
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
        name: "Field Recordings CD1.flac"
    )

    /// One entry of a sheet: its title and the seconds its slice of the
    /// audio runs. The release lists no tracks, so no entry has a listed
    /// length.
    struct CueEntry {
        let title: String
        let seconds: UInt64
    }

    static let fieldRecordingsEntries = [
        CueEntry(title: "Slow Morning", seconds: 622),
        CueEntry(title: "Weather Report Card", seconds: 276),
        CueEntry(title: "Night Ferry", seconds: 276),
    ]

    /// A CUE sheet of Field Recordings' folder, on disc `number`, asking for
    /// its disc's audio file, bound as `bound` says.
    static func fieldRecordingsSheet(
        _ number: Int,
        bound: BridgeSheetBound
    ) -> BridgeSheetGroup {
        let name = "Field Recordings CD\(number).cue"
        let boundTo: BridgeMappingContainer? =
            if case .describes(let container) = bound {
                container
            }
            else {
                nil
            }
        let reference = BridgeSheetReferenceOptions(
            fileReference: "Field Recordings CD\(number).flac",
            bound: boundTo,
            options: [
                BridgeSheetBindingOption(
                    fileId: fieldRecordingsContainer.fileId,
                    name: fieldRecordingsContainer.fileId,
                    offer: .offered
                )
            ]
        )
        return BridgeSheetGroup(
            sheetId: name,
            name: name,
            bound: bound,
            boundAudio: reference.bound == nil
                ? .none : .one(reference: reference),
            unboundReferences: reference.bound == nil ? [reference] : nil,
            assigned: .disc(UInt32(number)),
            sideChoices: BridgeSheetSideChoice.discs(2)
        )
    }

    /// The first sheet's entries as mapping rows, each slice starting where
    /// the one before it ends.
    static func fieldRecordingsMappings(
        edit: BridgeRawReleaseEdit
    ) -> [BridgeTrackTableRow] {
        let container = fieldRecordingsContainer
        var startSample: UInt64 = 0
        return fieldRecordingsEntries.enumerated()
            .map { index, entry in
                let from = startSample
                startSample += entry.seconds * 44_100
                return tableRow(
                    edit.tracks[index],
                    source: .sheetSlice(
                        start: BridgeDurationClock.length(
                            ms: from * 1_000 / 44_100
                        )
                    ),
                    audition: .preview(
                        target: BridgePreviewTarget(
                            path: fakePath(container.name),
                            name: container.name,
                            startSample: from,
                            endSample: startSample
                        )
                    ),
                    position: "\(index + 1)",
                    lengthMs: entry.seconds * 1_000
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
                    .alone(name: "Harbor Records", catalogNumber: "HR-0779")
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

    /// The catalog supplies the cover because the scanned folder has no image files.
    static func fieldRecordingsCatalogCover() throws
        -> BridgeCoverChoice
    {
        let cover = BridgeRemoteImageSet(
            url: try BoardArtwork.fileURL(named: "Field Recordings").path,
            downscaled: []
        )
        return BoardArtwork.catalogCoverChoice(cover, source: .musicBrainz)
    }

    /// Field Recordings: two CD sheets, the first bound to its audio and
    /// carving three tracks, the second waiting for the audio it names. The
    /// release lists no tracks.
    @MainActor
    static func fieldRecordingsCandidate(cover: BridgeCoverChoice)
        -> ImportPaneFixture
    {
        let sheetOne = fieldRecordingsSheet(
            1,
            bound: .describes(container: fieldRecordingsContainer)
        )
        let sheetTwo = fieldRecordingsSheet(
            2,
            bound: .unresolved(requested: ["Field Recordings CD2.flac"])
        )
        let edit = fieldRecordingsEdit()
        // The folder's audio is the two discs' images, which one format
        // describes.
        var files = boardFolderFiles(tracks: [], images: [], log: false)
        files.sourceAudio = .uniform(
            descriptor: BridgeSourceAudioDescriptor(
                layout: .cue,
                format: previewSourceAudioFormat
            )
        )
        return paneFixture(
            folder: boardFolder(.fieldRecordings, files: files),
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
                    BridgeTrackTableSide(
                        side: .flat,
                        runs: [
                            .sheet(
                                sheet: sheetOne,
                                rows: fieldRecordingsMappings(edit: edit)
                            ),
                            .sheet(sheet: sheetTwo, rows: []),
                        ]
                    )
                ],
                files: [],
                untitledTrackCount: 0
            ),
            cover: cover,
            placement: .pending(
                folderCheck: .sourceTracksUnknown,
                records: boardRecords("board-field-recordings")
            ),
            live: triageLive(boardDraftActions),
            albumArtists: boardLibraryArtists("Mara Quill")
        )
    }

    // MARK: - Copper Sky and Winter Kitchen

    /// A folder no catalog has: the title and artist its files' tags give,
    /// which the run's title search asked by, and the catalog number its rip
    /// log states.
    struct NotFoundFolder {
        let folder: BoardFolder
        let album: String
        let artist: String
        let catalogNumber: String
    }

    static let copperSky = NotFoundFolder(
        folder: .copperSky,
        album: "Copper Sky",
        artist: "Odile Fen",
        catalogNumber: "FR-12"
    )

    static let winterKitchen = NotFoundFolder(
        folder: .winterKitchen,
        album: "Winter Kitchen",
        artist: "Odile Fen",
        catalogNumber: "ODF-7"
    )

    /// The two sides each folder no catalog has holds, titled by their tags.
    static let notFoundTracks = [
        BoardTrack(file: "01 Side A.flac", title: "Side A", seconds: 1_284),
        BoardTrack(file: "02 Side B.flac", title: "Side B", seconds: 1_342),
    ]

    static func notFoundCover(_ found: NotFoundFolder) throws
        -> BridgeMappingImage
    {
        try boardImage("\(found.album).jpg", art: found.album)
    }

    /// `found`'s folder with the draft its files' tags give, on the pane as
    /// the person left it in `session`.
    @MainActor
    static func notFoundCandidate(
        _ found: NotFoundFolder,
        image: BridgeMappingImage,
        session: CandidatePaneState
    ) -> ImportPaneFixture {
        let edit = boardEdit(
            title: found.album,
            artist: found.artist,
            year: "",
            tracks: notFoundTracks.enumerated()
                .map {
                    BoardTrackRow(
                        id: "\(found.folder.path)-\($0.offset)",
                        title: $0.element.title,
                        fileId: $0.element.file
                    )
                }
        )
        return paneFixture(
            folder: boardFolder(
                found.folder,
                files: boardFolderFiles(
                    tracks: notFoundTracks,
                    images: [image]
                )
            ),
            metadataProvenance: .fileMetadata,
            edit: edit,
            mapping: BridgeMappingTable(
                images: [image],
                trackSides: boardTrackSides(notFoundTracks, edit: edit),
                files: [],
                untitledTrackCount: 0
            ),
            cover: boardCover(image),
            session: session,
            live: needsYou(.notFound),
            albumArtists: boardLibraryArtists(found.artist)
        )
    }

    /// The identify run that found nothing for `found`, asking `providers`:
    /// the folder has no Disc ID and no barcode, and no provider matched its
    /// catalog number or its title.
    static func notFoundRun(
        _ found: NotFoundFolder,
        providers: [BridgeLookupCatalog]
    ) -> BridgeIdentifyRun {
        let noMatch = providers.map {
            BridgeProviderCell(source: $0, lookup: .noMatch)
        }
        return BridgeIdentifyRun(
            providers: providers,
            discId: .absent,
            barcode: .absent,
            catalog: .numbers(
                rows: [
                    BridgeSignalValueRow(
                        value: found.catalogNumber,
                        excluded: false,
                        cells: noMatch
                    )
                ],
                candidates: []
            ),
            isrc: .absent,
            search: .searched(
                album: found.album,
                artist: found.artist,
                cells: noMatch
            ),
            readingArtwork: false
        )
    }

    /// What `found`'s pane shows: identification found nothing asking
    /// `providers`, and `search` is the typed search out for it, if any.
    static func notFoundRuntime(
        _ found: NotFoundFolder,
        providers: [BridgeLookupCatalog],
        search: BridgeCandidateSearch? = nil
    ) -> BridgeCandidateRuntimeSnapshot {
        BridgeCandidateRuntimeSnapshot(
            identifyState: .state(
                state: .notFoundAnywhere(
                    run: notFoundRun(found, providers: providers)
                )
            ),
            inFlight: search.map {
                BridgeCandidateInFlight(import: nil, search: $0, signals: nil)
            }
        )
    }

    /// Copper Sky's typed search, as the sources answer it: MusicBrainz has
    /// its album in two pressings, and Discogs has not answered.
    static let copperSkySearch: BridgeCandidateSearch = {
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
            titles: [copperSky.album],
            artists: [.named(copperSky.artist)],
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
            query: .general(artist: copperSky.artist, album: copperSky.album),
            unusableBarcode: nil,
            sources: searchSources(
                musicbrainz: .done(count: 2),
                discogs: .searching
            ),
            groups: [group],
            status: .searching
        )
    }()

    // MARK: - Skipped

    /// Harbor Waltz as its read answers it: its one image, then its other
    /// files, two of its audio files damaged.
    static func boardHarborWaltzDetail(
        candidate: BridgeInvalidCandidate,
        image: BridgeMappingImage
    ) -> BridgeInvalidCandidateDetail {
        BridgeInvalidCandidateDetail(
            candidate: candidate,
            images: [image],
            files: [
                mappingFile(
                    previewFile(
                        name: "01 Harbor Waltz.flac",
                        size: 28_400_000,
                        role: .audio
                    ),
                    role: .audio
                ),
                mappingFile(
                    previewFile(
                        name: "02 Night Shift.flac",
                        size: 0,
                        role: .audio
                    ),
                    role: .damagedAudio(damage: .empty)
                ),
                mappingFile(
                    previewFile(
                        name: "03 Tide Table.flac",
                        size: 31_200_000,
                        role: .audio
                    ),
                    role: .damagedAudio(damage: .unreadable)
                ),
                mappingFile(
                    previewFile(
                        name: "04 Last Ferry.flac",
                        size: 26_900_000,
                        role: .audio
                    ),
                    role: .audio
                ),
                mappingFile(
                    previewFile(
                        name: "notes.txt",
                        size: 2_000,
                        role: .document
                    ),
                    role: .document
                ),
            ]
        )
    }
}
