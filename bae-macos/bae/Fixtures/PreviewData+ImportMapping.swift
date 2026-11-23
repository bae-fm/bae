import AppKit
import BaeKit
import Foundation

@testable import bae

/// Fixtures for the import mapping table and its candidate states.
extension PreviewData {
    // MARK: - The mapping table

    private static let sourceAudioFormat = BridgeAudioFormat(
        codec: "FLAC",
        sampleRateHz: 44_100,
        bitsPerSample: 16,
        bitrateKbps: nil,
        channels: 2
    )

    /// Where a fake file named `name` would be, as the bridge carries a
    /// path.
    static func fakePath(_ name: String) -> Data {
        URL(fileURLWithPath: "/tmp/fake/\(name)").pathBytes
    }

    /// The whole of the folder's file `name`, as the preview player plays
    /// it.
    static func wholeFile(_ name: String) -> BridgeTrackAudition {
        .preview(
            target: BridgePreviewTarget(
                path: fakePath(name),
                name: name,
                startSample: 0,
                endSample: nil
            )
        )
    }

    /// A row of the track table, its listed length shown where it is
    /// `listedMs`.
    static func tableRow(
        _ track: BridgeRawTrackEdit,
        source: BridgeTrackSource,
        audition: BridgeTrackAudition,
        position: String,
        lengthMs: UInt64,
        listedMs: UInt64? = nil,
        artist: BridgeTrackArtistDisplay = .albumArtist
    ) -> BridgeTrackTableRow {
        BridgeTrackTableRow(
            track: track,
            source: source,
            audition: audition,
            position: position,
            length: BridgeDurationClock.length(ms: lengthMs),
            listedLength: listedMs.map { BridgeDurationClock.length(ms: $0) },
            artist: artist
        )
    }

    static func mappingFile(
        _ file: BridgeCandidateFile,
        role: BridgeMappingRole
    ) -> BridgeMappingFile {
        BridgeMappingFile(
            fileId: file.file.name,
            name: file.file.name,
            size: file.file.size,
            previewTarget: role == .audio
                ? BridgePreviewTarget(
                    path: fakePath(file.file.name),
                    name: file.file.fileName,
                    startSample: 0,
                    endSample: nil
                ) : nil,
            role: role
        )
    }

    /// One audio file and the track the release puts on it.
    private static func mappingTrackRow(
        _ index: Int
    ) -> BridgeTrackTableRun {
        let name = "Track \(index).flac"
        let lengthMs = UInt64(180_000 + index * 15000)
        return .track(
            row: tableRow(
                confirmEditValues.tracks[index - 1],
                source: .file(name: name),
                audition: wholeFile(name),
                position: "\(index)",
                lengthMs: lengthMs,
                // Track 4's release lists a longer length than its audio
                // measures.
                listedMs: index == 4 ? lengthMs + 21000 : nil
            )
        )
    }

    /// One of the folder's files that is not one of the release's tracks.
    private static func carriedRow(
        _ file: BridgeCandidateFile,
        role: BridgeMappingRole
    ) -> BridgeMappingFileRow {
        .file(file: mappingFile(file, role: role))
    }

    /// The folder's images, as the gallery shows them.
    static let mappingImages: [BridgeMappingImage] = [
        mappingImage(coverImage),
        mappingImage(backImage),
        mappingImage(scanImage),
    ]

    static func mappingImage(
        _ file: BridgeCandidateFile,
    ) -> BridgeMappingImage {
        BridgeMappingImage(
            fileId: file.file.name,
            name: file.file.name,
            file: previewArtFile(file.file.fileName),
        )
    }

    /// The mapping the picked release produces: nine files against nine
    /// tracks, every row paired, with the folder's images, which the gallery
    /// shows and the files leave out, and documents carried beside them.
    static let mappingTable = BridgeMappingTable(
        images: mappingImages,
        trackSides: [
            BridgeTrackTableSide(
                side: .flat,
                runs: (1...9).map(mappingTrackRow)
            )
        ],
        files: [
            carriedRow(infoLog, role: .document)
        ],
        untitledTrackCount: 0
    )

    private static let moreTracksAudio = BridgeCandidateFile(
        file: BridgeFileInfo(
            name: "Album Image.flac",
            size: 221_100_000,
            fileName: "Album Image.flac",
            audioFormat: BridgeAudioFormat(
                codec: "FLAC",
                sampleRateHz: 44_100,
                bitsPerSample: 16,
                bitrateKbps: nil,
                channels: 2
            )
        ),
        role: .audio
    )

    static let moreTracksCandidateFiles = BridgeCandidateFiles(
        fileMetadataIdentity: "more-tracks-audio",
        files: [moreTracksAudio],
        coverFiles: [],
        sourceAudio: .uniform(
            descriptor: BridgeSourceAudioDescriptor(
                layout: .file,
                format: sourceAudioFormat
            )
        )
    )

    static let moreTracksEditValues: BridgeRawReleaseEdit = {
        var edit = editMetadataDraft(trackCount: 1)
        edit.tracks[0].file = .standalone(fileId: moreTracksAudio.file.name)
        edit.tracks[0].title = moreTracksAudio.file.fileName
        return edit
    }()

    private static let moreTracksReleaseDetail = PreviewPickedRelease(
        releaseId: "rel-more-tracks",
        source: .musicBrainz,
        title: moreTracksEditValues.albumTitle,
        defaultCover: nil
    )

    /// Ignoring a CUE replaces its songs with the whole container.
    static let moreTracksMappingTable = BridgeMappingTable(
        images: [],
        trackSides: [
            BridgeTrackTableSide(
                side: .flat,
                runs: [
                    .track(
                        row: tableRow(
                            moreTracksEditValues.tracks[0],
                            source: .file(name: moreTracksAudio.file.name),
                            audition: wholeFile(moreTracksAudio.file.name),
                            position: "1",
                            lengthMs: 2_720_000
                        )
                    )
                ]
            )
        ],
        files: [],
        untitledTrackCount: 0
    )

    /// One entry the folder's sheet carves out of its single container.
    private static func sheetEntryMapping(
        _ index: Int
    ) -> BridgeTrackTableRow {
        tableRow(
            confirmEditValues.tracks[index],
            source: .sheetSlice(
                start: BridgeDurationClock.length(ms: UInt64(index) * 1_000)
            ),
            audition: .preview(
                target: BridgePreviewTarget(
                    path: fakePath(mappedAudioContainer.file.name),
                    name: mappedAudioContainer.file.fileName,
                    startSample: UInt64(index) * 44_100,
                    endSample: UInt64(index + 1) * 44_100
                )
            ),
            position: "\(index + 1)",
            lengthMs: UInt64(180_000 + (index + 1) * 15000)
        )
    }

    private static let previewSheetGroup = BridgeSheetGroup(
        sheetId: boundTrackSheet.file.name,
        name: boundTrackSheet.file.fileName,
        bound: .describes(
            container: BridgeMappingContainer(
                fileId: mappedAudioContainer.file.name,
                name: mappedAudioContainer.file.fileName
            )
        ),
        boundAudio: .one(reference: sheetReferenceOptions),
        unboundReferences: nil,
        assigned: .disc(1),
        sideChoices: BridgeSheetSideChoice.discs(2)
    )

    /// Every row kind the table draws, with the folder's gallery beside it:
    /// the sheet heading the nine entries it carves, the two documents
    /// carried with the release, the diagnostic logs collapsed to the one
    /// row their shared role makes of them, a damaged image, and one
    /// non-audio video. What
    /// the Import-tab shot reads, so a change to any row kind shows up
    /// in the canvas without hunting for the fixture that has it.
    static let everyRowKindMappingTable = BridgeMappingTable(
        images: mappingImages,
        trackSides: [
            BridgeTrackTableSide(
                side: .flat,
                runs: [
                    .sheet(
                        sheet: previewSheetGroup,
                        rows: (0..<9).map(sheetEntryMapping)
                    )
                ]
            )
        ],
        files: [
            carriedRow(infoLog, role: .document),
            carriedRow(notesDocument, role: .document),
        ]
            + previewLogDocuments.map {
                carriedRow($0, role: .document)
            }
            + [
                carriedRow(damagedFrontImage, role: .damagedImage),
                carriedRow(supplementalVideo, role: .other),
            ],
        untitledTrackCount: 0
    )

    /// A library release's files that no track plays from, as its editor
    /// lists them: its rip log, its track sheet and a scan the import found
    /// damaged, in the release's file order.
    static let libraryReleaseFiles: [BridgeMappingFile] = [
        libraryReleaseFile("back.png", size: 512, role: .damagedImage),
        libraryReleaseFile("disc.cue", size: 1_400, role: .document),
        libraryReleaseFile("rip.log", size: 4_000, role: .document),
    ]

    private static func libraryReleaseFile(
        _ name: String,
        size: UInt64,
        role: BridgeMappingRole
    ) -> BridgeMappingFile {
        BridgeMappingFile(
            fileId: "release-file-\(name)",
            name: name,
            size: size,
            previewTarget: nil,
            role: role
        )
    }

    // MARK: - Mapping pane candidates

    private static func mappingFolder(
        name: String,
        files: BridgeCandidateFiles
    ) -> BridgeFolderCandidate {
        BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/\(name)".utf8),
            folderPath: Data("/Music/Downloads/\(name)".utf8),
            sourceFolderName: name,
            watchedFolderPath: importWatchedFolderPath,
            files: files,
            trackCount: 9,
            skipped: false,
        )
    }

    /// A pane the person moved to `presentation`, with Find online on
    /// `section` and `artist` and `album` typed in its search form.
    static func paneSession(
        presentation: CandidateMetadataPresentation,
        section: FindOnlineSection = .automatic,
        artist: String = "",
        album: String = ""
    ) -> CandidatePaneState {
        CandidatePaneState(
            presentation: presentation,
            findOnlineSection: section,
            search: CandidateSearchState(
                searchArtist: artist,
                searchAlbum: album
            )
        )
    }

    /// The release lines core states of `edit`'s draft when its tracks play
    /// `minutes` together, or none: one label line for each label row.
    static func releaseLines(
        of edit: BridgeRawReleaseEdit,
        minutes: UInt64?
    ) -> BridgeDraftReleaseLines {
        let pressing = edit.pressing
        return BridgeDraftReleaseLines(
            year: pressing.year.isEmpty ? nil : pressing.year,
            labels: pressing.labels.map {
                BridgeLabelLine(
                    names: [$0.label.name],
                    catalogNumbers: [$0.label.catalogNumber]
                )
            },
            areaTerms: bridgeAreaTerms(facts: pressing.facts),
            mediaTerms: bridgeMediaTerms(media: pressing.facts.media),
            pressingDetails: bridgePressingDetails(facts: pressing.facts),
            totalDuration: minutes.map { .minutesOnly(minutes: $0) }
        )
    }

    struct ImportPaneFixture {
        let detail: BridgeImportCandidateDetail
        var session = CandidatePaneState()

        @MainActor
        var candidate: Candidate {
            var value = Candidate(detail: detail)
            value.pane = session
            return value
        }
    }

    /// A candidate as the per-candidate read answers for it: the folder,
    /// where the queue places it, what runs for it, and everything the pane
    /// draws. The pane holds none of it — this is the one value it renders
    /// from.
    @MainActor
    static func paneFixture(
        folder: BridgeFolderCandidate,
        metadataProvenance: BridgeMetadataProvenance? = nil,
        releaseLink: BridgeReleaseLink? = nil,
        edit: BridgeRawReleaseEdit = blankDraftValues,
        releaseLines: BridgeDraftReleaseLines? = nil,
        mapping: BridgeMappingTable,
        cover: BridgeCoverChoice? = nil,
        failure: BridgeError? = nil,
        session: CandidatePaneState = CandidatePaneState(),
        placement: BridgeCandidatePanePlacement = .pending(
            folderCheck: nil,
            records: nil
        ),
        live: BridgeCandidateLiveState? = nil,
        importStatus: BridgeCandidateImportStatus? = nil,
        libraryStatus: BridgeLibraryStatus? = nil,
        album: BridgeDraftAlbum = .new,
        albumArtists: BridgeArtistField? = nil
    ) -> ImportPaneFixture {
        ImportPaneFixture(
            detail: BridgeImportCandidateDetail(
                candidate: folder,
                partCount: 0,
                actionable: true,
                placement: placement,
                live: live
                    ?? BridgeCandidateLiveState(
                        identifying: nil,
                        import: nil,
                        actions: (metadataProvenance == nil
                            && edit.albumTitle.isEmpty ? [] : [.import])
                            + [
                                .identify, .resetToFileMetadata,
                                .clearMetadata, .skip, .revealFolder,
                            ],
                        standing: .notLookedUp,
                        badge: nil
                    ),
                importStatus: importStatus,
                signals: nil,
                stage: .drafting(
                    draft: BridgeCandidateDraftPane(
                        pickedLibraryStatus: libraryStatus,
                        album: album,
                        metadataDraft: edit,
                        pressingOffers: pressingOffers,
                        releaseLines: releaseLines,
                        albumArtists: albumArtists,
                        metadataDraftIsBlank: edit.albumTitle.isEmpty,
                        metadataProvenance: metadataProvenance,
                        releaseLink: releaseLink,
                        mapping: mapping,
                        cover: cover,
                        failure: failure,
                        paneFailure: nil,
                        opensOn: .draft
                    )
                )
            ),
            session: session
        )
    }

    /// A candidate with a release selected: the metadata card states what
    /// it is, the mapping table pairs nine files with nine tracks,
    /// and the commit bar counts them.
    @MainActor
    static let mappingCandidate: Candidate = paneFixture(
        folder: mappingFolder(
            name: "Album Title One",
            files: candidateFilesTracks
        ),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: releaseDetailBridge.source,
                key: releaseDetailBridge.releaseId
            )
        ),
        releaseLink: .pressing(
            link: BridgePressingLink(
                record: BridgeLookupRef(
                    catalog: releaseDetailBridge.source,
                    key: releaseDetailBridge.releaseId
                ),
                partners: []
            )
        ),
        edit: confirmEditValues,
        mapping: mappingTable,
        cover: releaseDetailBridge.defaultCover,
        albumArtists: editMetadataAlbumArtists,
    )
    .candidate

    /// The applied release remains recorded after its CUE is ignored;
    /// the track list contains the whole file with its own metadata.
    @MainActor
    static let moreTracksMappingCandidate: Candidate = paneFixture(
        folder: BridgeFolderCandidate(
            parts: [],
            candidateKey: Data("/Music/Downloads/Partial Album".utf8),
            folderPath: Data("/Music/Downloads/Partial Album".utf8),
            sourceFolderName: "Partial Album",
            watchedFolderPath: importWatchedFolderPath,
            files: moreTracksCandidateFiles,
            trackCount: 1,
            skipped: false,
        ),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: moreTracksReleaseDetail.source,
                key: moreTracksReleaseDetail.releaseId
            )
        ),
        releaseLink: .pressing(
            link: BridgePressingLink(
                record: BridgeLookupRef(
                    catalog: moreTracksReleaseDetail.source,
                    key: moreTracksReleaseDetail.releaseId
                ),
                partners: []
            )
        ),
        edit: moreTracksEditValues,
        mapping: moreTracksMappingTable,
        cover: moreTracksReleaseDetail.defaultCover,
        albumArtists: editMetadataAlbumArtists,
    )
    .candidate

    /// The candidate the Import-tab shot selects: the settled release
    /// read off the CUE+FLAC folder it came from, with every row kind in
    /// its table. Keyed to `folderCandidates`' first, so the row the
    /// sidebar selects and the pane behind it are the same folder.
    @MainActor
    static let importTabCandidate: Candidate = paneFixture(
        folder: mappingFolder(
            name: "Album Title One",
            files: bridgeCandidateFiles
        ),
        metadataProvenance: .externalRelease(
            record: BridgeLookupRef(
                catalog: releaseDetailBridge.source,
                key: releaseDetailBridge.releaseId
            )
        ),
        releaseLink: .pressing(
            link: BridgePressingLink(
                record: BridgeLookupRef(
                    catalog: releaseDetailBridge.source,
                    key: releaseDetailBridge.releaseId
                ),
                partners: []
            )
        ),
        edit: confirmEditValues,
        mapping: everyRowKindMappingTable,
        cover: releaseDetailBridge.defaultCover,
        albumArtists: editMetadataAlbumArtists,
    )
    .candidate

    /// Direct entry starts with blank editable metadata while retaining the
    /// candidate's physical audio-to-track mapping.
    private static let blankDraftValues = blankDraft(
        files: (1...9).map { "Track \($0).flac" }
    )

    /// A draft with no metadata over `files`, each file the audio of an
    /// untitled track.
    static func blankDraft(files: [String]) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "",
            albumArtistAssignments: [],
            albumYear: "",
            pressing: BridgeRawPressingEdit(
                year: "",
                labels: [],
                barcodes: [],
                facts: PreviewData.pressingFacts()
            ),
            tracks: files.enumerated()
                .map { index, file in
                    BridgeRawTrackEdit(
                        id: "draft-track-\(index)",
                        title: "",
                        artistAssignments: .albumArtists,
                        file: .standalone(fileId: file)
                    )
                }
        )
    }
}

extension BridgeMediumSide {
    /// The first side of the medium `medium`, the one side of a disc that is
    /// not a record or cassette.
    static func firstOf(_ medium: UInt32) -> BridgeMediumSide {
        BridgeMediumSide(medium: medium, side: 1)
    }
}

extension BridgeSheetSideChoice {
    /// The disc `number` of a release whose mediums are not records or
    /// cassettes, named as core names it.
    static func disc(_ number: UInt32) -> BridgeSheetSideChoice {
        BridgeSheetSideChoice(
            side: .firstOf(number),
            heading: .disc(disc: number)
        )
    }

    /// The choices of a release of `count` discs that are not records or
    /// cassettes, each named as its disc, as core offers them.
    static func discs(_ count: UInt32) -> [BridgeSheetSideChoice] {
        (1...count).map(disc)
    }
}
