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
    static let candidateKey = "/Music/Downloads/Walkthrough"
    static let releaseId = "rel-walkthrough"
    static let source: BridgeMetadataSource = .musicBrainz
    static let provenance: BridgeMetadataProvenance = .externalRelease(
        source: source,
        releaseId: releaseId,
        partners: []
    )

    static let audioFormat = BridgeAudioFormat(
        codec: "FLAC",
        sampleRateHz: 44_100,
        bitsPerSample: 16,
        bitrateKbps: nil,
        channels: 2
    )

    static func newArtist(_ name: String) -> BridgeArtistAssignment {
        .new(
            seed: BridgeNewArtistSeed(
                name: name,
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            )
        )
    }
}

extension MappingFixtures {
    // MARK: - Thirteen files, twelve tracks

    static func audioFile(_ index: Int) -> BridgeMappingFile {
        BridgeMappingFile(
            fileId: "\(index).flac",
            name: "\(index).flac",
            size: UInt64(30_000_000 + index * 1_000_000),
            localPath: "/tmp/walkthrough/\(index).flac",
            previewTarget: BridgePreviewTarget(
                path: "/tmp/walkthrough/\(index).flac",
                startSample: 0,
                endSample: nil
            ),
            durationMs: UInt64(200_000 + index * 1000),
            audioFormat: audioFormat,
            role: .audio,
            alternatives: [.audio, .notATrack],
            roleChoice: .audio
        )
    }

    static func trackEdit(
        _ index: Int,
        title: String,
        file: BridgeAudioFile?
    ) -> BridgeRawTrackEdit {
        BridgeRawTrackEdit(
            id: "import-track-\(index)",
            title: title,
            artistAssignments: .albumArtists,
            side: 1,
            trackNumber: nil,
            file: file
        )
    }

    /// One loose audio file and the track the release puts on it.
    static func pairedRow(_ index: Int) -> BridgeTrackMapping {
        BridgeTrackMapping(
            source: .file(file: audioFile(index)),
            becomes: .track(
                track: trackEdit(
                    index - 1,
                    title: "Track \(index)",
                    file: .standalone(fileId: "\(index).flac")
                ),
                position: "\(index)",
                namedBySource: true
            ),
            durationMs: UInt64(200_000 + index * 1000)
        )
    }

    static func flatSection(
        _ mappings: [BridgeTrackMapping]
    ) -> BridgeMappingTrackSection {
        BridgeMappingTrackSection(
            side: .flat,
            headerKey: nil,
            content: .tracks(mappings: mappings)
        )
    }

    static let thirteenFileTable = thirteenFileTable(lastTitle: "")

    /// The same thirteen rows with the last one named — what the next read
    /// answers with once that row has been written.
    static func thirteenFileTable(lastTitle: String) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            trackSections: [
                flatSection(
                    (1...12).map(pairedRow)
                        + [
                            BridgeTrackMapping(
                                source: .file(file: audioFile(13)),
                                becomes: .track(
                                    track: trackEdit(
                                        12,
                                        title: lastTitle,
                                        file: .standalone(fileId: "13.flac")
                                    ),
                                    position: "13",
                                    namedBySource: false
                                ),
                                durationMs: audioFile(13).durationMs
                            )
                        ]
                )
            ],
            files: [],
            reconciliation: .moreFiles(files: 13, tracks: 12)
        )
    }

    // MARK: - One container, before and after its sheet is bound

    static let containerId = "Album Title.flac"
    static let sheetId = "Album Title.cue"
    static let containerPath = "/tmp/walkthrough/Album Title.flac"

    static let containerFile = BridgeMappingFile(
        fileId: containerId,
        name: containerId,
        size: 380_000_000,
        localPath: containerPath,
        previewTarget: BridgePreviewTarget(
            path: containerPath,
            startSample: 0,
            endSample: nil
        ),
        durationMs: 2_400_000,
        audioFormat: audioFormat,
        role: .audio,
        alternatives: [.audio, .notATrack],
        roleChoice: .audio
    )

    static let container = BridgeMappingContainer(
        fileId: containerId,
        name: containerId,
        size: 380_000_000,
        audioFormat: audioFormat
    )

    static func sheetGroup(
        container: BridgeMappingContainer?,
        assignment: BridgeSheetDisc
    ) -> BridgeSheetGroup {
        BridgeSheetGroup(
            sheetId: sheetId,
            name: sheetId,
            size: 2_048,
            localPath: "/tmp/walkthrough/\(sheetId)",
            bound: container.map { .describes(container: $0) }
                ?? .unresolved(requested: [containerId]),
            assignment: assignment,
            discOptions: [1, 2]
        )
    }

    /// A track the release names that the folder has nothing for.
    static func missingRow(_ index: Int) -> BridgeTrackMapping {
        BridgeTrackMapping(
            source: .missing,
            becomes: .track(
                track: trackEdit(
                    index,
                    title: "Track \(index + 1)",
                    file: nil
                ),
                position: "\(index + 1)",
                namedBySource: true
            ),
            durationMs: UInt64(200_000 + index * 1000)
        )
    }

    /// The container as one loose audio file taking the release's first track,
    /// with the other eleven left with nothing behind them.
    private static var looseContainerTrackSections: [BridgeMappingTrackSection]
    {
        [
            flatSection(
                [
                    BridgeTrackMapping(
                        source: .file(file: containerFile),
                        becomes: .track(
                            track: trackEdit(
                                0,
                                title: "Track 1",
                                file: .standalone(fileId: containerId)
                            ),
                            position: "1",
                            namedBySource: true
                        ),
                        durationMs: 201_000
                    )
                ] + (1..<12).map(missingRow)
            )
        ]
    }

    /// The sheet describes nothing, so it carves nothing.
    static let unboundSheetTable = BridgeMappingTable(
        images: [],
        trackSections: looseContainerTrackSections,
        files: [
            .sheet(
                sheet: sheetGroup(
                    container: nil,
                    assignment: .disc(number: 1)
                )
            )
        ],
        reconciliation: .moreTracks(files: 1, tracks: 12)
    )

    /// An ignored sheet speaks for nothing either, so its container is loose
    /// audio again.
    static let ignoredSheetTable = BridgeMappingTable(
        images: [],
        trackSections: looseContainerTrackSections,
        files: [
            .sheet(
                sheet: sheetGroup(container: container, assignment: .ignored)
            )
        ],
        reconciliation: .moreTracks(files: 1, tracks: 12)
    )

    /// One entry of the bound sheet, carved out of the container.
    static func entry(_ index: Int) -> BridgeTrackMapping {
        let startSample = UInt64(index) * 200 * 44_100
        let previewTarget = BridgePreviewTarget(
            path: containerPath,
            startSample: startSample,
            endSample: startSample + 200 * 44_100
        )
        return BridgeTrackMapping(
            source: .sheetEntry(
                entry: BridgeMappingEntry(
                    sheetId: sheetId,
                    index: UInt32(index),
                    number: UInt32(index + 1),
                    title: "Sheet Track \(index + 1)",
                    durationMs: UInt64(200_000 + index * 1000),
                    containerId: containerId,
                    containerName: containerId,
                    containerLocalPath: containerPath,
                    previewTarget: previewTarget,
                    audioFormat: audioFormat
                )
            ),
            becomes: .track(
                track: trackEdit(
                    index,
                    title: "Track \(index + 1)",
                    file: .sheetSlice(
                        fileId: containerId,
                        sheetId: sheetId,
                        index: UInt32(index)
                    )
                ),
                position: "\(index + 1)",
                namedBySource: true
            ),
            durationMs: UInt64(200_000 + index * 1000)
        )
    }

    /// The same folder once the sheet is bound: twelve entries out of one file.
    static func boundSheetTable(
        assignment: BridgeSheetDisc = .disc(number: 1)
    ) -> BridgeMappingTable {
        BridgeMappingTable(
            images: [],
            trackSections: [
                BridgeMappingTrackSection(
                    side: .flat,
                    headerKey: nil,
                    content: .sheet(
                        sheet: sheetGroup(
                            container: container,
                            assignment: assignment
                        ),
                        entries: (0..<12).map(entry)
                    )
                )
            ],
            files: [],
            reconciliation: .agrees(count: 12)
        )
    }

    /// What the folder's own tags say it is: two tracks, no release behind
    /// them, so the table carries no tally.
    static let fileTagsTable = BridgeMappingTable(
        images: [],
        trackSections: [
            flatSection(
                (1...2)
                    .map { index in
                        BridgeTrackMapping(
                            source: .file(file: audioFile(index)),
                            becomes: .track(
                                track: BridgeRawTrackEdit(
                                    id: "file-tags-track-\(index - 1)",
                                    title: "Track \(index)",
                                    artistAssignments: .albumArtists,
                                    side: 1,
                                    trackNumber: Int32(index),
                                    file: .standalone(fileId: "\(index).flac")
                                ),
                                position: "\(index)",
                                namedBySource: true
                            ),
                            durationMs: audioFile(index).durationMs
                        )
                    }
            )
        ],
        files: [],
        reconciliation: nil
    )
}

extension MappingFixtures {
    // MARK: - The album fields alongside the table

    static let albumSeed = BridgeReleaseUserEdit(
        albumTitle: "Album Title",
        albumArtistAssignments: [newArtist("Artist Name")],
        albumYear: 1987,
        pressing: BridgePressingEdit(
            year: 1996,
            format: "CD",
            label: nil,
            catalogNumber: nil,
            country: nil,
            barcode: nil
        ),
        tracks: []
    )

    static let albumEdit = BridgeRawReleaseEdit(
        albumTitle: "Album Title",
        albumArtistAssignments: [newArtist("Artist Name")],
        albumYear: "1987",
        pressing: BridgeRawPressingEdit(
            year: "1996",
            format: "CD",
            label: "",
            catalogNumber: "",
            country: "",
            barcode: ""
        ),
        tracks: []
    )

    static let blankEdit = BridgeRawReleaseEdit(
        albumTitle: "",
        albumArtistAssignments: [],
        albumYear: "",
        pressing: BridgeRawPressingEdit(
            year: "",
            format: "",
            label: "",
            catalogNumber: "",
            country: "",
            barcode: ""
        ),
        tracks: []
    )

    /// The release the fixture folder is picked as, as its documents describe
    /// it.
    static let releaseDetail = BridgeReleaseDetail(
        releaseId: releaseId,
        source: source,
        sourceGroupId: nil,
        title: "Album Title",
        artist: "Artist Name",
        year: 1996,
        format: "CD",
        label: nil,
        catalogNumber: nil,
        country: nil,
        barcode: nil,
        trackCount: 12,
        tracks: [],
        coverArt: [],
        defaultCover: nil
    )

    /// The pressing the fixture release states — what claiming it exactly is a
    /// claim about, and what an edit is read against.
    static let exactPressing = BridgeRawPressingEdit(
        year: "1996",
        format: "CD",
        label: "",
        catalogNumber: "",
        country: "",
        barcode: ""
    )

    static let emptyFiles = BridgeCandidateFiles(
        fileTagsIdentity: "empty-audio-files",
        files: [],
        sourceAudio: nil
    )

    /// The value the per-candidate read answers with for the fixture folder:
    /// picked as the release above, with `mapping` as its table.
    @MainActor
    static func detail(
        mapping: BridgeMappingTable?,
        edit: BridgeRawReleaseEdit = albumEdit,
        metadataProvenance: BridgeMetadataProvenance? = provenance,
        metadataRevision: UInt64 = 1,
        initialMetadataSource: BridgeDefaultImportMetadataSource = .none,
        failure: BridgeImportFailure? = nil,
        presentation: BridgeMetadataPresentation = .draft,
        candidateKey key: String = MappingFixtures.candidateKey,
        folderName: String = "Walkthrough"
    ) -> BridgeImportCandidateDetail {
        let folder = BridgeFolderCandidate(
            folderPath: key,
            sourceFolderName: folderName,
            watchedFolderPath: "/Music/Downloads",
            files: emptyFiles,
            trackCount: 13,
            skipped: false,
            isAdded: false
        )
        return BridgeImportCandidateDetail(
            candidate: folder,
            actionable: true,
            resumedIdentifyState: .idle,
            row: BridgeTriageRow(
                candidateKey: key,
                folderName: folderName,
                watchedFolderPath: "/Music/Downloads",
                displayPath: folderName,
                resolvedBoundaries: [],
                combineAncestorKey: nil,
                actionable: true,
                placement: metadataProvenance == nil && edit.albumTitle.isEmpty
                    ? .pending : .ready,
                skipAction: .skip,
                matched: nil,
                metadataSummary: nil,
                coverThumbnail: nil,
                selectable: !edit.albumTitle.isEmpty,
                importStatus: nil,
                metadataProvenance: metadataProvenance
            ),
            release: {
                if case .externalRelease = metadataProvenance {
                    return releaseDetail
                }
                return nil
            }(),
            pickedLibraryStatus: nil,
            fileEvidence: [],
            metadataDraft: edit,
            metadataDraftIsBlank: edit.albumTitle.isEmpty,
            metadataProvenance: metadataProvenance,
            metadataRevision: metadataRevision,
            initialMetadataSource: initialMetadataSource,
            mapping: mapping
                ?? BridgeMappingTable(
                    images: [],
                    trackSections: [],
                    files: [],
                    reconciliation: nil
                ),
            cover: nil,
            signals: nil,
            failure: failure,
            session: session(presentation: presentation)
        )
    }

    /// A pane session with an empty form and no banner, on `presentation`.
    static func session(
        presentation: BridgeMetadataPresentation = .draft
    ) -> BridgeCandidateSession {
        BridgeCandidateSession(
            presentation: presentation,
            search: BridgeSearchForm(
                tab: .general,
                artist: "",
                album: "",
                catalog: "",
                barcode: ""
            ),
            error: nil
        )
    }

    /// A store holding one folder candidate read as the release picked for it,
    /// with `mapping` as the table core answers with.
    @MainActor
    static func store(
        mapping: BridgeMappingTable?,
        metadataProvenance: BridgeMetadataProvenance? = provenance,
        edit: BridgeRawReleaseEdit = albumEdit,
        initialMetadataSource: BridgeDefaultImportMetadataSource = .none,
        presentation: BridgeMetadataPresentation = .draft
    ) -> ImportStore {
        let store = ImportStore()
        store.applyCandidateDetail(
            key: candidateKey,
            detail: detail(
                mapping: mapping,
                edit: edit,
                metadataProvenance: metadataProvenance,
                initialMetadataSource: initialMetadataSource,
                presentation: presentation
            )
        )
        return store
    }

    /// The mapping table the store's one candidate holds.
    @MainActor
    static func mapping(of store: ImportStore) -> BridgeMappingTable {
        store.selectedCandidates[candidateKey]?.mapping
            ?? BridgeMappingTable(
                images: [],
                trackSections: [],
                files: [],
                reconciliation: nil
            )
    }

    /// Whether bae-core can shape what the pane would commit into a savable
    /// release — the only thing standing between the commit bar's button and an
    /// import.
    @MainActor
    static func isCommittable(_ store: ImportStore) -> Bool {
        guard let candidate = store.selectedCandidates[candidateKey],
            var edit = candidate.edit
        else { return false }
        edit.tracks = bridgeMappingTracks(table: candidate.mapping)
        if case .valid = shapeReleaseEdit(raw: edit) { return true }
        return false
    }
}
