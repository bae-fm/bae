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
        ShotScene(
            id: "mac.album.edit",
            size: albumEditorSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(stage: .editor, release: filledRelease)
        },
        ShotScene(
            id: "mac.album.edit.cover",
            size: albumEditorSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(stage: .coverSheet)
        },
        ShotScene(
            id: "mac.album.edit.album-picker",
            size: albumEditorSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(stage: .albumMenu)
        },
        ShotScene(
            id: "mac.album.edit.cue-image",
            size: albumEditorSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(stage: .editor, release: cueRelease(.original))
        },
        ShotScene(
            id: "mac.album.edit.reading-file",
            size: albumEditorSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(stage: .saving, release: cueRelease(.rebound))
        },
        // Its board draws the window at the album page boards' height.
        ShotScene(
            id: "mac.album.edit.drop-tracks",
            size: CGSize(width: 1_200, height: 1_020),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumEditorStage(
                stage: .dropTracksAlert,
                release: ignoredCueRelease,
                size: CGSize(width: 1_200, height: 1_020)
            )
        },
    ]

    /// The window the editor's other boards draw, which fits in the shot
    /// display's free area.
    private static let albumEditorSize = CGSize(width: 1_200, height: 1_100)

    private enum AlbumEditorStage {
        case editor
        case coverSheet
        case albumMenu
        /// The editor's Save runs and never finishes.
        case saving
        case dropTracksAlert
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

    /// The album page behind the editor: Northbound, the first card of the
    /// board's grid, with its four tracks and its cover.
    static var northboundAlbumDetail: BridgeAlbumDetail {
        var detail = PreviewData.boardAlbumDetail(
            title: "Northbound",
            year: 1974,
            catalogNumber: "HR-0412",
            tracks: PreviewData.northboundTracks.map { ($0.title, $0.seconds) }
        )
        let image = cover("Northbound")
        detail.album.cover = image
        detail.releases[0].cover = image
        detail.shownRelease.cover = image
        // The bar plays a track of Low Tide Hours, so none of these tracks
        // is the current one, as the boards draw them.
        var release = detail.releases[0]
        for index in release.tracks.indices {
            release.tracks[index].id = "northbound-track-\(index)"
        }
        release.trackGroups = release.trackGroups.map { group in
            var group = group
            group.tracks = release.tracks
            return group
        }
        detail.releases = [release]
        detail.shownRelease = release
        return detail
    }

    /// The release the editor opens on: Northbound as its import made it,
    /// with a change made, so the editor offers Cancel and Save, and the rip
    /// log its folder carried.
    private static var editedRelease: PreviewReleaseEdit {
        var release = PreviewData.northboundRelease(
            cover: cover("Northbound")
        )
        release.draft.files = PreviewData.carriedBoardFiles.map {
            .file(file: $0)
        }
        // The scene's images serve a release file by the art its id names, so
        // the file `cover.jpg` is the art `Northbound`.
        release.draft.images = [
            BridgeReleaseImage(
                fileId: "Northbound",
                name: "cover.jpg",
                source: .releaseFile(fileId: "Northbound"),
                key: .file(fileId: "Northbound")
            )
        ]
        release.draft.changed = true
        return release
    }

    /// The edited release with every field the editor offers filled, as the
    /// first board draws it: a second label, a barcode, two details, and
    /// the second track by an artist of its own.
    private static var filledRelease: PreviewReleaseEdit {
        var release = editedRelease
        var pressing = release.values.pressing
        // The heading is on the first label row and Add under the last.
        pressing.labels[0].place = BridgeListPlace(
            labelled: true,
            addsRow: false
        )
        pressing.labels.append(
            BridgeLabelRow(
                label: BridgeRawLabelEdit(
                    name: "Tidewater Music",
                    catalogNumber: "TW-88"
                ),
                place: BridgeListPlace(labelled: false, addsRow: true)
            )
        )
        pressing.barcodes = [.alone("0 41234 56789 2")]
        pressing.facts.discogsDetails = [.reissue, .remastered]
        release.values.pressing = pressing
        let artist = "Odile Fen"
        release.values.tracks[1].artistAssignments = .explicit(
            assignments: [PreviewData.boardArtist(artist)]
        )
        if case .track(var row)? = release.draft.sides?[0].runs[1] {
            row.track = release.values.tracks[1]
            row.artist = .own(field: PreviewData.boardLibraryArtists(artist))
            release.draft.sides?[0].runs[1] = .track(row: row)
        }
        return release
    }

    /// The same release as one image a CUE sheet carves the four tracks
    /// from: one run under the sheet and the image's name, each row giving
    /// where its track starts in the image, and the third its listed length.
    static func cueRelease(_ example: CueExample) -> PreviewReleaseEdit {
        let bound = example == .original ? cueImage : cueRip
        let other = example == .original ? cueRip : cueImage
        let container = BridgeMappingContainer(
            fileId: bound.id,
            name: bound.name
        )
        var release = editedRelease
        let sheet = cueSheet(bound: container)
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
                    audition: example == .importing
                        ? .preview(
                            target: BridgePreviewTarget(
                                path: PreviewData.fakePath(bound.name),
                                name: bound.name,
                                startSample: start * 44_100 / 1_000,
                                endSample: startMs * 44_100 / 1_000
                            )
                        )
                        : .libraryTrack,
                    position: "\(index + 1)",
                    lengthMs: track.durationMs,
                    listedMs: example != .original && index == 2 ? 327_000 : nil
                )
            }
        release.draft.sides = [
            BridgeTrackTableSide(
                side: .flat,
                runs: [.sheet(sheet: sheet, rows: rows)]
            )
        ]
        release.draft.files = [
            .file(file: carriedAudio(other.name)),
            .file(
                file: PreviewData.mappingFile(
                    PreviewData.previewFile(
                        name: "Northbound.log",
                        size: 4_000,
                        role: .document
                    ),
                    role: .document
                )
            ),
        ]
        if example == .rebound {
            release.draft.filesToRead = [bound.name]
        }
        return release
    }

    private static func cueSheet(bound container: BridgeMappingContainer)
        -> BridgeSheetGroup
    {
        let line = BridgeSheetReferenceOptions(
            fileReference: cueImage.name,
            bound: container,
            options: [cueImage, cueRip]
                .map { file in
                    BridgeSheetBindingOption(
                        fileId: file.id,
                        name: file.name,
                        offer: .offered
                    )
                }
        )
        return BridgeSheetGroup(
            sheetId: "Northbound.cue",
            name: "Northbound.cue",
            bound: .describes(container: container),
            boundAudio: .one(reference: line),
            unboundReferences: nil,
            assigned: .disc(1),
            sideChoices: BridgeSheetSideChoice.discs(1)
        )
    }

    enum CueExample { case original, rebound, importing }

    private static var ignoredCueRelease: PreviewReleaseEdit {
        var release = cueRelease(.rebound)
        release.draft.files?.removeFirst()
        release.draft.filesToRead = nil
        PreviewReleaseLibrary.ignoreSheet("Northbound.cue", in: &release)
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

    @MainActor
    private static func albumEditorStage(
        stage: AlbumEditorStage,
        release: PreviewReleaseEdit = editedRelease,
        size: CGSize = albumEditorSize
    ) -> ShotStage {
        let uiStore = UiStore()
        let detailAnswered = AlbumDetailAnswered()
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            albumCount: 4,
            albums: boardAlbums(first: ("Northbound", 1974)),
            openAlbum: true,
            detailAnswered: detailAnswered,
            detail: northboundAlbumDetail
        )
        let images = coverGalleryImages(of: "Northbound")
        let session = editorSession(
            release: release,
            saving: stage == .saving
        )
        presentEditor(
            on: uiStore,
            images: images,
            session: session,
            asksToDropTracks: stage == .dropTracksAlert
        )
        if stage == .coverSheet {
            presentCoverSheet(on: uiStore, images: images)
        }
        let window = ZStack(alignment: .topLeading) {
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                playbackStore: boardPlayback(),
                images: images,
                size: size
            )
            if stage == .albumMenu {
                albumMenu
            }
        }
        return ShotStage(window, images: images) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
                && detailAnswered.answered
                && (stage != .saving || session.readingLine != nil)
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

    /// The editor in its sheet; when `asksToDropTracks`, with the alert that
    /// asks before Save drops tracks, which the editor raises over its sheet.
    @MainActor
    private static func presentEditor(
        on uiStore: UiStore,
        images: ImageStore,
        session: ReleaseMetadataEditSession,
        asksToDropTracks: Bool
    ) {
        uiStore.presentSizedModal { host in
            let editor = ReleaseEditorSheet(
                session: session,
                displayName: "Northbound",
                host: host,
                onSaved: { _ in },
                onClose: {}
            )
            .environment(images)
            if asksToDropTracks {
                OverWindow.alert(DropTracksConfirmation.alert {}, over: editor)
            }
            else {
                editor
            }
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
                    BoardRemoteCover(position: 1, type: .front)
                ],
                files: ["cover.jpg", "back.jpg"],
                releaseId: editedReleaseId
            )
        )
        uiStore.presentSizedModal { host in
            CoverGalleryFrame(host: host) {
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
        .padding(.top, 278)
    }
}
