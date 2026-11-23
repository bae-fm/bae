import BaeKit
import Foundation

@testable import bae

/// The walkthrough folder the mapping-pane tests read: thirteen audio files
/// against a release that names twelve tracks, plus the CUE+FLAC shape of the
/// same release — one container a track sheet carves twelve entries out of.
///
/// Everything here is a bridge value, built the way core builds it, so the
/// tests exercise the pane's own reading of the mapping table rather than a
/// convenience shape invented for them.
enum MappingFixtures {
    static let candidateKey = Data("/Music/Downloads/Walkthrough".utf8)
    static let releaseId = "rel-walkthrough"
    static let source: BridgeLookupCatalog = .musicBrainz
    static let provenance: BridgeMetadataProvenance = .externalRelease(
        record: BridgeLookupRef(catalog: source, key: releaseId)
    )
    /// The pick of the fixture's release, and the link it leaves.
    static let link = BridgePressingLink(
        record: BridgeLookupRef(catalog: source, key: releaseId),
        partners: []
    )
    static let releaseLink = BridgeReleaseLink.pressing(link: link)

    static let audioFormat = BridgeAudioFormat(
        codec: "FLAC",
        sampleRateHz: 44_100,
        bitsPerSample: 16,
        bitrateKbps: nil,
        channels: 2
    )

    static func artistCredit(_ name: String) -> BridgeCredited {
        .typed(name)
    }
}

extension MappingFixtures {
    // MARK: - Thirteen files, twelve tracks

    /// The track `track` at `position`, `lengthMs` long, that plays the
    /// folder's file `name` at `path` whole.
    static func fileRow(
        _ name: String,
        path: Data,
        track: BridgeRawTrackEdit,
        position: String,
        lengthMs: UInt64
    ) -> BridgeTrackTableRun {
        .track(
            row: BridgeTrackTableRow(
                track: track,
                source: .file(name: name),
                audition: .preview(
                    target: BridgePreviewTarget(
                        path: path,
                        name: name,
                        startSample: 0,
                        endSample: nil
                    )
                ),
                position: position,
                length: BridgeDurationClock.length(ms: lengthMs),
                listedLength: nil,
                artists: nil,
                showsAlbumArtist: true
            )
        )
    }

    /// The folder's audio file numbered `index`, the track `track` plays
    /// whole, `lengthMs` long.
    static func audioRow(
        _ index: Int,
        track: BridgeRawTrackEdit,
        lengthMs: UInt64
    ) -> BridgeTrackTableRun {
        fileRow(
            "\(index).flac",
            path: Data("/tmp/walkthrough/\(index).flac".utf8),
            track: track,
            position: "\(index)",
            lengthMs: lengthMs
        )
    }

    static func trackEdit(
        _ index: Int,
        title: String,
        file: BridgeAudioFile
    ) -> BridgeRawTrackEdit {
        BridgeRawTrackEdit(
            id: "import-track-\(index)",
            title: title,
            artistAssignments: .albumArtists,
            file: file
        )
    }

    /// One loose audio file and the track the release puts on it.
    static func pairedRow(_ index: Int) -> BridgeTrackTableRun {
        audioRow(
            index,
            track: trackEdit(
                index - 1,
                title: "Track \(index)",
                file: .standalone(fileId: "\(index).flac")
            ),
            lengthMs: UInt64(200_000 + index * 1000)
        )
    }

    static func flatSide(
        _ runs: [BridgeTrackTableRun]
    ) -> BridgeTrackTableSide {
        BridgeTrackTableSide(side: .flat, runs: runs)
    }

    static let thirteenFileTable = thirteenFileTable(lastTitle: "")

    /// The same thirteen rows with the last one named — what the next read
    /// answers with once that row has been written.
    static func thirteenFileTable(lastTitle: String) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            trackSides: [
                flatSide(
                    (1...12).map(pairedRow)
                        + [
                            audioRow(
                                13,
                                track: trackEdit(
                                    12,
                                    title: lastTitle,
                                    file: .standalone(fileId: "13.flac")
                                ),
                                lengthMs: 213_000
                            )
                        ]
                )
            ],
            files: [],
            untitledTrackCount: lastTitle.isEmpty ? 1 : 0
        )
    }

    // MARK: - One container, before and after its sheet is bound

    static let containerId = "Album Title.flac"
    static let sheetId = "Album Title.cue"
    static let containerPath = Data("/tmp/walkthrough/Album Title.flac".utf8)

    static let container = BridgeMappingContainer(
        fileId: containerId,
        name: containerId
    )

    static func sheetGroup(
        container: BridgeMappingContainer?,
        assigned: BridgeSheetSideChoice?
    ) -> BridgeSheetGroup {
        let reference = BridgeSheetReferenceOptions(
            fileReference: containerId,
            bound: container,
            options: [
                BridgeSheetBindingOption(
                    fileId: containerId,
                    name: containerId,
                    offer: .offered
                )
            ]
        )
        return BridgeSheetGroup(
            sheetId: sheetId,
            name: sheetId,
            bound: container.map { .describes(container: $0) }
                ?? .unresolved(requested: [containerId]),
            boundAudio: reference.bound == nil
                ? .none : .one(reference: reference),
            unboundReferences: reference.bound == nil ? [reference] : nil,
            assigned: assigned,
            sideChoices: BridgeSheetSideChoice.discs(2)
        )
    }

    /// Ignoring the sheet exposes the entire container as one track.
    private static var looseContainerTrackSides: [BridgeTrackTableSide] {
        [
            flatSide(
                [
                    fileRow(
                        containerId,
                        path: containerPath,
                        track: trackEdit(
                            0,
                            title: containerId,
                            file: .standalone(fileId: containerId)
                        ),
                        position: "1",
                        lengthMs: 2_400_000
                    )
                ]
            )
        ]
    }

    /// The sheet describes nothing, so it carves nothing.
    static let unboundSheetTable = BridgeMappingTable(
        images: [],
        trackSides: looseContainerTrackSides,
        files: [
            .sheet(
                sheet: sheetGroup(
                    container: nil,
                    assigned: nil
                )
            )
        ],
        untitledTrackCount: 0
    )

    /// An ignored sheet speaks for nothing either, so its container is loose
    /// audio again.
    static let ignoredSheetTable = BridgeMappingTable(
        images: [],
        trackSides: looseContainerTrackSides,
        files: [
            .sheet(
                sheet: sheetGroup(container: container, assigned: nil)
            )
        ],
        untitledTrackCount: 0
    )

    /// One entry of the bound sheet, carved out of the container.
    static func entry(_ index: Int) -> BridgeTrackTableRow {
        let startSample = UInt64(index) * 200 * 44_100
        let previewTarget = BridgePreviewTarget(
            path: containerPath,
            name: "Album Title.flac",
            startSample: startSample,
            endSample: startSample + 200 * 44_100
        )
        return BridgeTrackTableRow(
            track: trackEdit(
                index,
                title: "Track \(index + 1)",
                file: .sheetSlice(
                    fileId: containerId,
                    sheetId: sheetId,
                    index: UInt32(index)
                )
            ),
            source: .sheetSlice(
                start: BridgeDurationClock.length(ms: UInt64(index) * 200_000)
            ),
            audition: .preview(target: previewTarget),
            position: "\(index + 1)",
            length: BridgeDurationClock.length(
                ms: UInt64(200_000 + index * 1000)
            ),
            listedLength: nil,
            artists: nil,
            showsAlbumArtist: true
        )
    }

    /// The same folder once the sheet is bound: twelve entries out of one file.
    static func boundSheetTable(
        assigned: BridgeSheetSideChoice? = .disc(1)
    ) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            trackSides: [
                BridgeTrackTableSide(
                    side: .flat,
                    runs: [
                        .sheet(
                            sheet: sheetGroup(
                                container: container,
                                assigned: assigned
                            ),
                            rows: (0..<12).map(entry)
                        )
                    ]
                )
            ],
            files: [],
            untitledTrackCount: 0
        )
    }

    /// What the folder's own tags say it is: two tracks, no release behind
    /// them, so the table carries no tally.
    static let fileTagsTable = BridgeMappingTable(
        images: [],
        trackSides: [
            flatSide(
                (1...2)
                    .map { index in
                        audioRow(
                            index,
                            track: BridgeRawTrackEdit(
                                id: "file-tags-track-\(index - 1)",
                                title: "Track \(index)",
                                artistAssignments: .albumArtists,
                                file: .standalone(fileId: "\(index).flac")
                            ),
                            lengthMs: UInt64(200_000 + index * 1000)
                        )
                    }
            )
        ],
        files: [],
        untitledTrackCount: 0
    )
}

extension MappingFixtures {
    // MARK: - The album fields alongside the table

    static let albumEdit = BridgeRawReleaseEdit(
        albumTitle: "Album Title",
        albumArtistAssignments: [artistCredit("Artist Name")],
        albumYear: "1987",
        pressing: BridgeRawPressingEdit(
            year: "1996",
            labels: [],
            barcodes: [],
            facts: PreviewData.pressingFacts(
                country: nil,
                media: PreviewData.media(.cd)
            )
        ),
        tracks: []
    )

    static let blankEdit = BridgeRawReleaseEdit(
        albumTitle: "",
        albumArtistAssignments: [],
        albumYear: "",
        pressing: BridgeRawPressingEdit(
            year: "",
            labels: [],
            barcodes: [],
            facts: PreviewData.pressingFacts()
        ),
        tracks: []
    )

    static let emptyFiles = BridgeCandidateFiles(
        fileMetadataIdentity: "empty-audio-files",
        files: [],
        coverFiles: [],
        sourceAudio: nil
    )

    /// The value the per-candidate read answers with for the fixture folder:
    /// picked as the release above, with `mapping` as its table.
    @MainActor
    static func detail(
        mapping: BridgeMappingTable?,
        edit: BridgeRawReleaseEdit = albumEdit,
        metadataProvenance: BridgeMetadataProvenance? = provenance,
        releaseLink: BridgeReleaseLink? = releaseLink,
        failure: BridgeError? = nil,
        presentation: BridgeMetadataPresentation = .draft,
        candidateKey key: Data = MappingFixtures.candidateKey,
        folderName: String = "Walkthrough",
        audioIdentity: String = "empty-audio-files",
        records: [BridgeReleaseRecord] = []
    ) -> BridgeImportCandidateDetail {
        let folder = sourceFolder(
            key: key,
            name: folderName,
            audioIdentity: audioIdentity
        )
        return BridgeImportCandidateDetail(
            candidate: folder,
            partCount: 0,
            actionable: true,
            placement: .pending(folderCheck: nil, records: records),
            live: BridgeCandidateLiveState(
                identifying: nil,
                import: nil,
                actions: (metadataProvenance == nil && edit.albumTitle.isEmpty
                    ? [] : [.import])
                    + [
                        .identify, .resetToFileMetadata, .clearMetadata,
                        .skip, .revealFolder,
                    ],
                standing: .notLookedUp,
                badge: nil
            ),
            importStatus: nil,
            signals: nil,
            stage: .drafting(
                draft: BridgeCandidateDraftPane(
                    pickedLibraryStatus: nil,
                    album: .new,
                    metadataDraft: edit,
                    pressingOffers: PreviewData.pressingOffers,
                    releaseLines: nil,
                    albumArtists: nil,
                    metadataDraftIsBlank: edit.albumTitle.isEmpty,
                    metadataProvenance: metadataProvenance,
                    releaseLink: releaseLink,
                    mapping: mapping
                        ?? BridgeMappingTable(
                            images: [],
                            trackSides: [],
                            files: [],
                            untitledTrackCount: 0
                        ),
                    cover: nil,
                    failure: failure,
                    paneFailure: nil,
                    opensOn: presentation
                )
            )
        )
    }

    private static func sourceFolder(
        key: Data,
        name: String,
        audioIdentity: String
    ) -> BridgeFolderCandidate {
        BridgeFolderCandidate(
            parts: [],
            candidateKey: key,
            folderPath: key,
            sourceFolderName: name,
            watchedFolderPath: Data("/Music/Downloads".utf8),
            files: BridgeCandidateFiles(
                fileMetadataIdentity: audioIdentity,
                files: emptyFiles.files,
                coverFiles: emptyFiles.coverFiles,
                sourceAudio: emptyFiles.sourceAudio
            ),
            trackCount: 13,
            skipped: false,
        )
    }

    /// A store holding one folder candidate read as the release picked for it,
    /// with `mapping` as the table core answers with.
    @MainActor
    static func store(
        mapping: BridgeMappingTable?,
        metadataProvenance: BridgeMetadataProvenance? = provenance,
        releaseLink: BridgeReleaseLink? = releaseLink,
        edit: BridgeRawReleaseEdit = albumEdit,
        presentation: BridgeMetadataPresentation = .draft,
        records: [BridgeReleaseRecord] = []
    ) -> ImportStore {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: candidateKey,
            detail: detail(
                mapping: mapping,
                edit: edit,
                metadataProvenance: metadataProvenance,
                releaseLink: releaseLink,
                presentation: presentation,
                records: records
            )
        )
        return store
    }

    /// The mapping table the store's one candidate holds; `nil` until core
    /// read its draft.
    @MainActor
    static func mapping(of store: ImportStore) -> BridgeMappingTable? {
        store.selectedCandidates[candidateKey]?.mapping
    }

    /// Whether bae-library can shape what the pane would commit into a savable
    /// release — the only thing standing between the commit bar's button and an
    /// import.
    @MainActor
    static func isCommittable(_ store: ImportStore) -> Bool {
        guard let candidate = store.selectedCandidates[candidateKey],
            var edit = candidate.edit
        else { return false }
        edit.tracks = (candidate.mapping?.trackRows ?? []).map(\.track)
        if case .valid = shapeReleaseEdit(raw: edit) { return true }
        return false
    }
}
