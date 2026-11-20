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

    /// One of the folder's audio files, as the mapping table's left half
    /// shows it.
    private static func mappingAudio(_ index: Int) -> BridgeMappingFile {
        return BridgeMappingFile(
            fileId: "Track \(index).flac",
            name: "Track \(index).flac",
            size: UInt64(35_000_000 + index * 2_000_000),
            previewTarget: BridgePreviewTarget(
                path: fakePath("Track \(index).flac"),
                name: "Track \(index).flac",
                startSample: 0,
                endSample: nil
            ),
            role: .audio
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
    ) -> BridgeTrackMapping {
        BridgeTrackMapping(
            source: .file(file: mappingAudio(index)),
            track: confirmEditValues.tracks[index - 1],
            position: "\(index)",
            length: BridgeDurationClock.length(
                ms: UInt64(180_000 + index * 15000)
            ),
            // Track 4's release lists a longer length than its audio measures.
            listedLength: index == 4
                ? BridgeDurationClock.length(
                    ms: UInt64(180_000 + index * 15000 + 21000)
                )
                : nil,
            artistStanding: nil
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
    /// tracks, every row paired, with the folder's images, listed again among
    /// its files, and documents carried alongside them.
    static let mappingTable = BridgeMappingTable(
        images: mappingImages,
        trackSides: [
            BridgeMappingTrackSide(
                side: .flat,
                headerKey: nil,
                runs: [
                    .tracks(
                        mappings: (1...9).map(mappingTrackRow)
                    )
                ]
            )
        ],
        files: [
            carriedRow(infoLog, role: .document),
            carriedRow(coverImage, role: .image),
            carriedRow(backImage, role: .image),
            carriedRow(scanImage, role: .image),
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
            BridgeMappingTrackSide(
                side: .flat,
                headerKey: nil,
                runs: [
                    .tracks(mappings: [
                        BridgeTrackMapping(
                            source: .file(
                                file: BridgeMappingFile(
                                    fileId: moreTracksAudio.file.name,
                                    name: moreTracksAudio.file.fileName,
                                    size: moreTracksAudio.file.size,
                                    previewTarget: BridgePreviewTarget(
                                        path: fakePath(
                                            moreTracksAudio.file.name
                                        ),
                                        name: moreTracksAudio.file
                                            .fileName,
                                        startSample: 0,
                                        endSample: nil
                                    ),
                                    role: .audio
                                )
                            ),
                            track: moreTracksEditValues.tracks[0],
                            position: "1",
                            length: BridgeDurationClock.length(ms: 2_720_000),
                            listedLength: nil,
                            artistStanding: nil
                        )
                    ])
                ]
            )
        ],
        files: [],
        untitledTrackCount: 0
    )

    /// One entry the folder's sheet carves out of its single container.
    private static func sheetEntryMapping(
        _ index: Int
    ) -> BridgeTrackMapping {
        let durationMs = UInt64(180_000 + (index + 1) * 15000)
        let entry = BridgeMappingEntry(
            sheetId: boundTrackSheet.file.name,
            index: UInt32(index),
            number: UInt32(index + 1),
            start: BridgeDurationClock.length(ms: UInt64(index) * 1_000),
            containerId: mappedAudioContainer.file.name,
            previewTarget: BridgePreviewTarget(
                path: fakePath(mappedAudioContainer.file.name),
                name: mappedAudioContainer.file.fileName,
                startSample: UInt64(index) * 44_100,
                endSample: UInt64(index + 1) * 44_100
            )
        )
        return BridgeTrackMapping(
            source: .sheetEntry(entry: entry),
            track: confirmEditValues.tracks[index],
            position: "\(index + 1)",
            length: BridgeDurationClock.length(ms: durationMs),
            listedLength: nil,
            artistStanding: nil
        )
    }

    private static let previewSheetGroup = BridgeSheetGroup(
        sheetId: boundTrackSheet.file.name,
        name: boundTrackSheet.file.fileName,
        size: boundTrackSheet.file.size,
        bound: .describes(
            container: BridgeMappingContainer(
                fileId: mappedAudioContainer.file.name,
                name: mappedAudioContainer.file.fileName,
                size: mappedAudioContainer.file.size
            )
        ),
        boundAudio: .one(reference: sheetReferenceOptions),
        unboundReferences: nil,
        assignment: .disc(number: 1),
        discOptions: [1, 2]
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
            BridgeMappingTrackSide(
                side: .flat,
                headerKey: nil,
                runs: [
                    .sheet(
                        sheet: previewSheetGroup,
                        entries: (0..<9).map(sheetEntryMapping)
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
    /// lists them: its rip log, its track sheet, a cover image and a scan
    /// the import found damaged, in the release's file order.
    static let libraryReleaseFiles: [BridgeMappingFile] = [
        libraryReleaseFile("back.png", size: 512, role: .damagedImage),
        libraryReleaseFile("cover.jpg", size: 1_100_000, role: .image),
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

    /// A candidate as the per-candidate read answers for it: the folder,
    /// where the queue places it, what runs for it, and everything the pane
    /// draws. The pane holds none of it — this is the one value it renders
    /// from.
    @MainActor
    static func paneCandidate(
        folder: BridgeFolderCandidate,
        metadataProvenance: BridgeMetadataProvenance? = nil,
        releaseLink: BridgeReleaseLink? = nil,
        edit: BridgeRawReleaseEdit = blankDraftValues,
        mapping: BridgeMappingTable,
        cover: BridgeCoverChoice? = nil,
        failure: BridgeError? = nil,
        session: CandidatePaneState = CandidatePaneState(),
        placement: BridgeCandidatePanePlacement = .pending(
            folderCheck: nil,
            records: []
        ),
        live: BridgeCandidateLiveState? = nil,
        importStatus: BridgeCandidateImportStatus? = nil,
        libraryStatus: BridgeLibraryStatus? = nil,
        album: BridgeDraftAlbum = .new,
        albumArtistStanding: BridgeArtistFieldStanding? = nil
    ) -> Candidate {
        var candidate = Candidate(
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
                        pressingOffers: pressingOffers(for: edit.pressing),
                        albumArtistStanding: albumArtistStanding,
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
            )
        )
        candidate.pane = session
        return candidate
    }

    /// A candidate with a release selected: the metadata card states what
    /// it is, the mapping table pairs nine files with nine tracks,
    /// and the commit bar counts them.
    @MainActor
    static let mappingCandidate: Candidate = paneCandidate(
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
    )

    /// The applied release remains recorded after its CUE is ignored;
    /// the track list contains the whole file with its own metadata.
    @MainActor
    static let moreTracksMappingCandidate: Candidate = paneCandidate(
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
    )

    /// The candidate the Import-tab shot selects: the settled release
    /// read off the CUE+FLAC folder it came from, with every row kind in
    /// its table. Keyed to `folderCandidates`' first, so the row the
    /// sidebar selects and the pane behind it are the same folder.
    @MainActor
    static let importTabCandidate: Candidate = paneCandidate(
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
    )

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
