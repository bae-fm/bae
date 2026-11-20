import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The library's release editor over the album page, as the boards draw
    /// it: on its own, with Change cover over it, with its album pop-up open
    /// over its row, over a release a CUE sheet carves from one image, and
    /// saving that release after its sheet's Audio file pop-up picked the
    /// rip, while the save reads the rip.
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
        albumEditorScene(
            id: "mac.album.edit.reading-file",
            stage: .saving,
            release: reboundCueRelease
        ),
    ]

    private enum AlbumEditorStage {
        case editor
        case coverSheet
        case albumMenu
        /// The editor's Save runs and never finishes.
        case saving
    }

    /// The image the edited release's sheet carves, and the rip of it the
    /// release carries beside it.
    private static let cueImage = (
        id: "Northbound.flac", name: "Northbound.flac"
    )
    private static let cueRip = (
        id: "Northbound (2019 rip).flac",
        name: "Northbound (2019 rip).flac"
    )

    private static let editedReleaseId = "rel-board-0"

    /// The release the editor opens on: Northbound as its import made it,
    /// with a change made, so the editor offers Cancel and Save, and the rip
    /// log its folder carried.
    private static var editedRelease: PreviewReleaseEdit {
        var release = PreviewData.northboundRelease()
        release.draft.files = PreviewData.carriedBoardFiles
        release.draft.cover = BridgeImageRef(
            id: editedReleaseId,
            version: "1",
            imageType: .cover
        )
        release.draft.changed = true
        return release
    }

    /// The same release as one image a CUE sheet carves the four tracks
    /// from: one run under the sheet and the image's name, each row giving
    /// where its track starts in the image, and the third its listed length.
    private static var editedCueRelease: PreviewReleaseEdit {
        var release = editedRelease
        let line = BridgeSheetReferenceOptions(
            fileReference: cueImage.name,
            fileId: cueImage.id,
            options: [cueImage, cueRip]
                .map { file in
                    BridgeSheetBindingOption(
                        fileId: file.id,
                        name: file.name,
                        offer: .offered
                    )
                }
        )
        let sheet = BridgeSheetGroup(
            sheetId: "Northbound.cue",
            name: "Northbound.cue",
            bound: .describes(
                container: BridgeMappingContainer(
                    fileId: cueImage.id,
                    name: cueImage.name
                )
            ),
            boundAudio: .one(reference: line),
            unboundReferences: nil,
            assigned: .disc(1),
            sideChoices: BridgeSheetSideChoice.discs(1),
            ignorable: false
        )
        var startMs: UInt64 = 0
        let rows = PreviewData.northboundTracks.enumerated()
            .map { index, track in
                let start = startMs
                startMs += track.durationMs
                return PreviewData.tableRow(
                    release.values.tracks[index],
                    source: .sheetSlice(
                        start: BridgeDurationClock.length(ms: start)
                    ),
                    audition: .libraryTrack,
                    position: "\(index + 1)",
                    lengthMs: track.durationMs,
                    listedMs: index == 2 ? 327_000 : nil
                )
            }
        release.draft.sides = [
            BridgeTrackTableSide(
                side: .flat,
                runs: [.sheet(sheet: sheet, rows: rows)]
            )
        ]
        release.draft.files = [
            carriedAudio(cueRip.name),
            PreviewData.mappingFile(
                PreviewData.previewFile(
                    name: "Northbound.log",
                    size: 4_000,
                    role: .document
                ),
                role: .document
            ),
        ]
        return release
    }

    /// The CUE release with its sheet's FILE line moved onto the rip: the run
    /// names the rip, the pop-up checks it, the Files list holds the image
    /// in its place, and a save reads the rip first.
    private static var reboundCueRelease: PreviewReleaseEdit {
        var release = editedCueRelease
        release.draft.sides = release.draft.sides?
            .map { side in
                var side = side
                side.runs = side.runs.map { run in
                    guard case .sheet(var sheet, let rows) = run,
                        case .one(var line) = sheet.boundAudio
                    else {
                        return run
                    }
                    line.fileId = cueRip.id
                    sheet.boundAudio = .one(reference: line)
                    sheet.bound = .describes(
                        container: BridgeMappingContainer(
                            fileId: cueRip.id,
                            name: cueRip.name
                        )
                    )
                    return .sheet(sheet: sheet, rows: rows)
                }
                return side
            }
        release.draft.files?[0] = carriedAudio(cueImage.name)
        release.draft.filesToRead = [cueRip.name]
        return release
    }

    /// The audio file `name` of the release, which no track plays from.
    private static func carriedAudio(_ name: String) -> BridgeMappingFile {
        PreviewData.mappingFile(
            PreviewData.previewFile(
                name: name,
                size: 121_000_000,
                role: .audio
            ),
            role: .audio
        )
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
            let images = coverGalleryImages(of: "Northbound")
            let session = editorSession(
                release: release,
                saving: stage == .saving
            )
            presentEditor(on: uiStore, images: images, session: session)
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
                    && (stage != .saving || session.readingLine != nil)
            }
        }
    }

    /// An editor's session on a library holding `release`; when `saving`,
    /// the session's Save runs and never finishes.
    @MainActor
    private static func editorSession(
        release: PreviewReleaseEdit,
        saving: Bool
    ) -> ReleaseMetadataEditSession {
        let session = PreviewData.releaseEditSession(
            releaseId: editedReleaseId,
            release: release,
            library: PreviewReleaseLibrary(
                release: release,
                holdsSaves: saving
            )
        )
        if saving {
            session.save { _ in }
        }
        return session
    }

    @MainActor
    private static func presentEditor(
        on uiStore: UiStore,
        images: ImageStore,
        session: ReleaseMetadataEditSession
    ) {
        uiStore.presentModal {
            ReleaseEditorSheet(
                session: session,
                displayName: "Northbound · CD · Harbor Records HR-0412 · 1974",
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
