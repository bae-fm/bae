import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macReleaseIdentityScenes: [ShotScene] = [
        identityScene("mac.component.release-identity.filled") {
            identity(
                lowTideEdit(),
                artists: PreviewData.boardLibraryArtists("Mara Quill"),
                album: libraryAlbum,
                audio: true,
                cover: try coverWell(true)
            )
        },
        identityScene("mac.component.release-identity.new-artist") {
            identity(
                lowTideEdit(),
                artists: newArtists,
                album: .new,
                audio: true,
                cover: try coverWell(true)
            )
        },
        identityScene(
            "mac.component.release-identity.cover-off-while-a-save-reads"
        ) {
            identity(
                lowTideEdit(),
                artists: PreviewData.boardLibraryArtists("Mara Quill"),
                album: libraryAlbum,
                audio: true,
                cover: try coverWell(true, canEdit: false)
            )
        },
        identityScene("mac.component.release-identity.no-cover") {
            identity(
                lowTideEdit(),
                artists: PreviewData.boardLibraryArtists("Mara Quill"),
                album: nil,
                audio: true,
                cover: try coverWell(false)
            )
        },
        identityScene("mac.component.release-identity.long-title") {
            identity(
                PreviewData.boardEdit(
                    title:
                        "Low Tide Hours and Other Songs from the Harbor at Night",
                    artist: "Mara Quill",
                    year: "1992",
                    tracks: []
                ),
                artists: PreviewData.boardLibraryArtists("Mara Quill"),
                album: .new,
                audio: true,
                cover: try coverWell(true)
            )
        },
        identityScene("mac.component.release-identity.nothing-under-it") {
            identity(
                lowTideEdit(),
                artists: PreviewData.boardLibraryArtists("Mara Quill"),
                album: nil,
                audio: false,
                cover: try coverWell(true)
            )
        },
        identityScene("mac.component.release-identity.empty") {
            identity(
                blankEdit(),
                artists: nil,
                album: nil,
                audio: false,
                cover: try coverWell(false)
            )
        },
    ]

    /// Low Tide Hours, a CD from Harbor Records, as the draft card holds it.
    private static func lowTideEdit() -> BridgeRawReleaseEdit {
        PreviewData.boardEdit(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            year: "1992",
            pressing: BridgeRawPressingEdit(
                year: "1992",
                labels: [
                    .alone(name: "Harbor Records", catalogNumber: "HR-1042")
                ],
                barcodes: [],
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd),
                    status: .official
                )
            ),
            tracks: []
        )
    }

    /// The scan's reading of the folder's audio, which the audio line shows.
    private static func folderAudio() -> BridgeSourceAudioSummary? {
        draftSummary().sourceAudio
    }

    /// What the draft card knows of the folder: its audio.
    private static func draftSummary() -> ImportReleaseSummary {
        let edit = lowTideEdit()
        let candidate = PreviewData.paneCandidate(
            folder: PreviewData.boardFolder(
                .lowTideHours,
                files: PreviewData.boardFolderFiles(
                    tracks: [PreviewData.northboundTracks[0]],
                    images: []
                )
            ),
            edit: edit,
            mapping: BridgeMappingTable(
                images: [],
                trackSides: [],
                files: [],
                untitledTrackCount: 0
            )
        )
        guard let draft = candidate.draft else {
            preconditionFailure("the pane candidate holds a draft")
        }
        return ImportReleaseSummary(candidate: candidate, draft: draft)
    }

    private static func coverWell(_ cover: Bool, canEdit: Bool = true) throws
        -> some View
    {
        ReleaseCoverWell(
            coverContent: cover
                ? .localFile(
                    try BoardArtwork.scannedFile(named: "Low Tide Hours")
                )
                : nil,
            onEditCover: cover ? {} : nil,
            onDropImage: nil
        )
        .disabled(!canEdit)
    }

    private static func identity<Cover: View>(
        _ values: BridgeRawReleaseEdit,
        artists: BridgeArtistField?,
        album: BridgeDraftAlbum?,
        audio: Bool,
        cover: Cover
    ) -> some View {
        let sourceAudio = audio ? folderAudio() : nil
        return importStores(
            MacReleaseIdentity(
                values: values,
                albumArtists: artists,
                writer: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                artistSearch: artistSearch(),
                cover: { cover },
                audioFacts: {
                    if let sourceAudio {
                        SourceAudioLine(sourceAudio: sourceAudio)
                    }
                },
                albumRow: {
                    if let album {
                        MacAlbumPicker(
                            album: album,
                            canEdit: true,
                            actions: inertAlbumActions
                        )
                    }
                }
            )
            .frame(width: 560, alignment: .leading)
        )
    }

    private static func identityScene<Identity: View>(
        _ id: String,
        height: CGFloat = 160,
        _ identity: @escaping @MainActor () throws -> Identity
    ) -> ShotScene {
        component(
            id,
            width: 560,
            height: height,
            images: PreviewData.artImageStore()
        ) {
            try identity()
        }
    }
}
