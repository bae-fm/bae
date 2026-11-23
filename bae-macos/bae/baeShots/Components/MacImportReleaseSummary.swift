import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportReleaseSummary`: identified, from the files' own tags, with
    /// no title yet, with files that disagree and with long values.
    static let macImportReleaseSummaryScenes: [ShotScene] = [
        releaseSummaryScene("mac.component.import-release-summary.identified") {
            releaseSummary(
                PreviewData.northboundEdit(),
                provenance: northboundRecord
            )
        },
        releaseSummaryScene(
            "mac.component.import-release-summary.from-file-metadata"
        ) {
            releaseSummary(
                PreviewData.northboundEdit(),
                provenance: .fileMetadata
            )
        },
        releaseSummaryScene("mac.component.import-release-summary.no-title-yet")
        {
            releaseSummary(
                PreviewData.blankDraft(files: []),
                provenance: nil,
                tracks: [],
                minutes: nil
            )
        },
        releaseSummaryScene(
            "mac.component.import-release-summary.files-disagree"
        ) {
            releaseSummary(
                PreviewData.northboundEdit(),
                provenance: northboundRecord,
                audio: .mixed(differences: [
                    .codec(codecs: ["FLAC", "MP3"]),
                    .sampleRate(sampleRatesHz: [44_100, 48_000]),
                ])
            )
        },
        releaseSummaryScene("mac.component.import-release-summary.long-values")
        {
            releaseSummary(
                longEdit(),
                provenance: northboundRecord,
                tracks: longTracks
            )
        },
    ]

    private static let flacAudio = BridgeSourceAudioSummary.uniform(
        descriptor: BridgeSourceAudioDescriptor(
            layout: .file,
            format: PreviewData.previewSourceAudioFormat
        )
    )

    /// The summary the import pane draws of Northbound's folder: `edit`'s
    /// draft, read as `provenance` says, over files whose audio is `audio` and
    /// whose tracks play `minutes` together, as the board states it.
    @MainActor
    private static func releaseSummary(
        _ edit: BridgeRawReleaseEdit,
        provenance: BridgeMetadataProvenance?,
        tracks: [PreviewData.BoardTrack] = PreviewData.northboundTracks,
        minutes: UInt64? = 18,
        audio: BridgeSourceAudioSummary? = flacAudio
    ) -> ImportReleaseSummary {
        var files = PreviewData.boardFolderFiles(tracks: tracks, images: [])
        files.sourceAudio = audio
        let candidate =
            PreviewData.paneFixture(
                folder: PreviewData.boardFolder(.northbound, files: files),
                metadataProvenance: provenance,
                edit: edit,
                releaseLines: provenance == .fileMetadata
                    ? nil
                    : PreviewData.releaseLines(of: edit, minutes: minutes),
                mapping: BridgeMappingTable(
                    images: [],
                    trackSides: PreviewData.boardTrackSides(
                        tracks,
                        edit: edit,
                        showsAlbumArtist: true
                    ),
                    files: [],
                    untitledTrackCount: 0
                )
            )
            .candidate
        guard let draft = candidate.draft else {
            preconditionFailure("the pane candidate holds a draft")
        }
        return ImportReleaseSummary(candidate: candidate, draft: draft)
    }

    private static let northboundRecord =
        BridgeMetadataProvenance
        .externalRelease(
            record: BridgeLookupRef(
                catalog: .musicBrainz,
                key: "board-northbound"
            )
        )

    private static func releaseSummaryScene(
        _ id: String,
        _ summary: @escaping @MainActor () -> ImportReleaseSummary
    ) -> ShotScene {
        component(id, width: 380, height: 110) {
            MacImportReleaseSummary(summary: summary(), style: .card)
                .frame(width: 360, alignment: .leading)
        }
    }

    /// A release of 24 tracks.
    private static let longTracks = (1...24)
        .map {
            PreviewData.BoardTrack(
                file: "\($0) Track.flac",
                title: "Track \($0)",
                seconds: 200
            )
        }

    /// Northbound's draft under a long title, a long artist line, a long
    /// catalog number and 24 tracks.
    private static func longEdit() -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title:
                "Songs for the Slow Orchards, Recorded Live at the Harbor Hall",
            artist: "The Slow Orchards and the Ines Varga Trio",
            year: "1974",
            pressing: BridgeRawPressingEdit(
                year: "1974",
                labels: [
                    .alone(
                        name: "Harbor Records",
                        catalogNumber: "HR-0412-A-DELUXE"
                    )
                ],
                barcodes: [],
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd),
                    status: .official
                )
            ),
            tracks: longTracks.enumerated()
                .map {
                    PreviewData.BoardTrackRow(
                        id: "long-\($0.offset)",
                        title: $0.element.title,
                        fileId: $0.element.file
                    )
                }
        )
    }
}
