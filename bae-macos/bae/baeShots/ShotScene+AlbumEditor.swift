import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The library's release editor over the album page, as the boards draw
    /// it: its cover opens Change cover over it, and its album pop-up opens
    /// over its row.
    static let albumEditorScenes: [ShotScene] = [
        albumEditorScene(id: "mac.album.edit.cover", stage: .coverSheet),
        albumEditorScene(
            id: "mac.album.edit.album-picker",
            stage: .albumMenu
        ),
    ]

    private enum AlbumEditorStage {
        case coverSheet
        case albumMenu
    }

    private static let editedReleaseId = "rel-board-0"

    /// The release the editor opens on: "Northbound" with a change made, so
    /// the editor offers Cancel and Save.
    private static var editedRelease: BridgeReleaseEditDraft {
        var draft = PreviewData.releaseEditDraft(trackCount: 4)
        draft.edit.albumTitle = "Northbound"
        draft.edit.albumYear = "1974"
        draft.edit.albumArtistAssignments = [
            PreviewData.pickedArtist("Mara Quill", artistId: "artist-quill")
        ]
        draft.edit.pressing.year = "1992"
        draft.edit.pressing.labels = [
            BridgeRawLabelEdit(name: "Harbor Records", catalogNumber: "HR-1042")
        ]
        draft.edit.pressing.barcodes = []
        let titles = ["Northbound", "Margins", "Salt Road", "Winter Light"]
        for index in draft.edit.tracks.indices {
            draft.edit.tracks[index].title = titles[index]
        }
        draft.cover = BridgeImageRef(
            id: editedReleaseId,
            version: "1",
            imageType: .cover
        )
        draft.album = .named(
            albumId: "album-northbound",
            title: "Northbound",
            artistLine: .named("Mara Quill")
        )
        draft.changed = true
        return draft
    }

    private static func albumEditorScene(
        id: String,
        stage: AlbumEditorStage
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
            presentEditor(on: uiStore, images: images)
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
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        }
    }

    @MainActor
    private static func presentEditor(on uiStore: UiStore, images: ImageStore) {
        let library = PreviewReleaseLibrary(draft: editedRelease)
        uiStore.presentModal {
            ReleaseEditorSheet(
                releaseId: editedReleaseId,
                displayName: "Northbound · CD · Harbor Records HR-1042 · 1992",
                opened: library.opened(),
                commands: ReleaseEditCommands(
                    open: { _ in library.opened() },
                    searchAlbums: { _ in [] },
                    catalogApply: { nil }
                ),
                onSaved: { _ in },
                onViewAlbum: { _ in },
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
            CoverPickerFrame {
                CoverGalleryView(
                    content: content,
                    remoteItems: .linked(content.remote?.items ?? []),
                    filter: .constant(.all),
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
                MacMenuItem(verbatim: "Northbound · Mara Quill", checked: true)
                {}
                MacMenuItem("New album") {}
                MacMenuItem("Show in library") {}
                MacMenuDivider()
                MacMenuItem("Choose another album…") {}
            },
            highlighted: 0,
            isPopUp: true
        )
        .padding(.leading, 258)
        .padding(.top, 302)
    }
}
