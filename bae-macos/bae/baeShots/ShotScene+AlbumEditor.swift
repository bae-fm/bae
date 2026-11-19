import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The library's release editor over the album page, as the boards draw
    /// it: on its own, with Change cover over it, with its album pop-up open
    /// over its row, and over a release a CUE sheet carves from one image.
    static let albumEditorScenes: [ShotScene] = [
        albumEditorScene(id: "mac.album.edit", stage: .editor),
        albumEditorScene(id: "mac.album.edit.cover", stage: .coverSheet),
        albumEditorScene(
            id: "mac.album.edit.album-picker",
            stage: .albumMenu
        ),
        albumEditorScene(
            id: "mac.album.edit.cue-image",
            stage: .editor,
            release: editedCueRelease
        ),
    ]

    private enum AlbumEditorStage {
        case editor
        case coverSheet
        case albumMenu
    }

    private static let editedReleaseId = "rel-board-0"

    /// The release the editor opens on: Northbound as its import made it,
    /// with a change made, so the editor offers Cancel and Save, and the rip
    /// log and front cover its folder carried.
    private static var editedRelease: PreviewReleaseEdit {
        var release = PreviewData.northboundRelease()
        release.draft.files = Array(PreviewData.carriedBoardFiles.prefix(2))
        release.draft.cover = BridgeImageRef(
            id: editedReleaseId,
            version: "1",
            imageType: .cover
        )
        release.draft.changed = true
        return release
    }

    /// The same release as one image a CUE sheet carves the four tracks
    /// from: one run under the image's name, each row giving where its track
    /// starts in the image.
    private static var editedCueRelease: PreviewReleaseEdit {
        var release = editedRelease
        let image = BridgeReleaseEditTrackSource(
            fileId: "Northbound.flac",
            name: "Northbound.flac",
            layout: .cue
        )
        var startMs: UInt64 = 0
        let rows = PreviewData.northboundTracks.enumerated()
            .map { index, track in
                let start = startMs
                startMs += track.durationMs
                return BridgeReleaseEditTrackRow(
                    edit: release.values.tracks[index],
                    context: BridgeReleaseEditTrackContext(
                        trackId: release.values.tracks[index].id,
                        source: image,
                        startClock: BridgeDurationClock.length(ms: start),
                        durationClock: BridgeDurationClock.length(
                            ms: track.durationMs
                        ),
                        positionText: "\(index + 1)"
                    ),
                    artistStanding: nil
                )
            }
        release.draft.sides = [
            BridgeReleaseEditTrackSide(
                side: .flat,
                headerKey: nil,
                runs: [.sheet(source: image, rows: rows)]
            )
        ]
        release.draft.files = [
            PreviewData.mappingFile(
                PreviewData.previewFile(
                    name: "Northbound.log",
                    size: 4_000,
                    role: .document
                ),
                role: .document
            ),
            PreviewData.carriedBoardFiles[1],
        ]
        return release
    }

    private static func albumEditorScene(
        id: String,
        stage: AlbumEditorStage,
        release: PreviewReleaseEdit = editedRelease
    ) -> ShotScene {
        let size = CGSize(width: 1_200, height: 1_120)
        return ShotScene(id: id, size: size) {
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore, openAlbum: true)
            let images = ImageStore(
                fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) },
                fetchReleaseImageBytes: { _, source in
                    switch source {
                    case .cover: PreviewData.previewArtData("Northbound")
                    case .releaseFile(let id): PreviewData.previewArtData(id)
                    }
                },
                fetchRemoteImage: { image, _ in
                    PreviewData.previewArtData(image.url)
                }
            )
            presentEditor(on: uiStore, images: images, release: release)
            if stage == .coverSheet {
                presentCoverSheet(on: uiStore, images: images)
            }
            return ShotStage(
                ZStack(alignment: .topLeading) {
                    ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: size
                    )
                    if stage == .albumMenu {
                        albumMenu
                    }
                },
                images: images
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        }
    }

    @MainActor
    private static func presentEditor(
        on uiStore: UiStore,
        images: ImageStore,
        release: PreviewReleaseEdit
    ) {
        let library = PreviewReleaseLibrary(release: release)
        uiStore.presentModal {
            ReleaseEditorSheet(
                releaseId: editedReleaseId,
                displayName: "Northbound · CD · Harbor Records HR-0412 · 1974",
                opened: library.opened(),
                commands: library.commands(),
                onSaved: { _ in },
                onClose: {}
            )
            .environment(images)
        }
    }

    @MainActor
    private static func presentCoverSheet(
        on uiStore: UiStore,
        images: ImageStore
    ) {
        let content = CoverGalleryContent(
            boardCoverGallery(
                remotes: [
                    BoardRemoteCover(name: "front", position: 1, type: .front)
                ],
                files: ["cover.jpg", "back.jpg"],
                releaseId: editedReleaseId
            )
        )
        uiStore.presentModal {
            CoverGalleryFrame {
                MacCoverGallery(
                    content: content,
                    remoteItems: .linked(content.remote?.items ?? []),
                    filter: .constant(.all),
                    focusedCover: .constant(nil),
                    selectedCover: nil,
                    lightboxLoad: LightboxImageLoad(),
                    onRefresh: {},
                    onFindRelease: {},
                    onSelect: { _ in },
                    onDone: {}
                )
            }
            .environment(images)
        }
    }

    /// The album pop-up open over its row, where the board draws it.
    private static var albumMenu: some View {
        MacMenuInPlace(
            {
                MacMenu(
                    MacAlbumPicker.albumOptions(editedRelease.draft.album),
                    selection: .standing,
                    select: { _ in }
                )
                .entries
            },
            highlighted: 0,
            isPopUp: true
        )
        .padding(.leading, 258)
        .padding(.top, 302)
    }
}
