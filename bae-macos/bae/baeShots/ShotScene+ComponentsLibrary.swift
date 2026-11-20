import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the library: the headings and the album
    /// tiles, the open album's pieces, the artist page's, the sort and the
    /// library heading's controls.
    static let libraryComponentScenes: [ShotScene] =
        headingScenes + albumTileScenes + albumPageScenes + artistPageScenes
        + sortScenes + libraryHeadingControlsScenes

    /// `MacSectionHeader` and `MacEyebrow`.
    private static let headingScenes: [ShotScene] = [
        component("mac.component.section-header.default", height: 70) {
            MacSectionHeader(title: "Works")
                .frame(width: 380, alignment: .leading)
        },
        component("mac.component.eyebrow.default", height: 50) {
            MacEyebrow(verbatim: "Side A")
        },
    ]

    /// The cover a scene's albums show: placeholder art named after the album.
    static func cover(_ title: String) -> BridgeImageRef {
        BridgeImageRef(id: title, version: "1", imageType: .cover)
    }

    static func coverStore() -> ImageStore {
        ImageStore(
            fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) }
        )
    }

    /// `MacAlbumTile` at rest, with its album open, selected, with no cover
    /// and with no year; and `MacArtistAlbumTile`.
    private static let albumTileScenes: [ShotScene] = [
        albumTileScene("mac.component.album-tile.resting"),
        albumTileScene("mac.component.album-tile.open", open: true),
        albumTileScene("mac.component.album-tile.selected", selected: true),
        albumTileScene("mac.component.album-tile.no-cover", covered: false),
        albumTileScene("mac.component.album-tile.no-year", year: nil),
        component(
            "mac.component.artist-album-tile.default",
            width: 540,
            height: 250,
            images: coverStore()
        ) {
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                artistAlbumCard("Low Tide Hours", 1992)
                artistAlbumCard("Field Recordings", 1979)
                artistAlbumCard("Northbound", 1974)
            }
        },
    ]

    private static func albumTileScene(
        _ id: String,
        open: Bool = false,
        selected: Bool = false,
        covered: Bool = true,
        year: Int32? = 1992
    ) -> ShotScene {
        component(
            id,
            width: 190,
            height: 260,
            images: coverStore()
        ) {
            albumTile(
                "Low Tide Hours",
                "Mara Quill",
                year,
                open: open,
                selected: selected,
                covered: covered,
                menu: AlbumCardMenu(
                    targetCount: 1,
                    onPlay: {},
                    onAddToQueue: {},
                    onAddNext: {},
                    onDelete: {}
                )
            )
        }
    }

    // MARK: - The open album's pieces

    private static func albumRating(
        _ id: String,
        _ rating: BridgeAlbumRating
    ) -> ShotScene {
        component(id, width: 300, height: 50) {
            MacAlbumRating(rating: rating, onRate: { _ in })
        }
    }

    /// `MacAlbumRating`: not rated, rated, and rated by others too.
    private static let albumRatingComponentScenes: [ShotScene] = [
        albumRating(
            "mac.component.album-rating.not-rated",
            BridgeAlbumRating(mine: nil, raters: [])
        ),
        albumRating(
            "mac.component.album-rating.rated",
            BridgeAlbumRating(mine: .four, raters: [])
        ),
        albumRating(
            "mac.component.album-rating.others-rated",
            ratedByYouAndAna
        ),
    ]

    private static func credit(
        _ role: String,
        _ artist: String,
        tracks: [String] = []
    ) -> BridgeCreditRow {
        BridgeCreditRow(
            role: .word(role),
            artists: .named(artist),
            tracks: tracks
        )
    }

    private static func albumCredits(
        _ id: String,
        height: CGFloat,
        _ rows: [BridgeCreditRow]
    ) -> ShotScene {
        component(id, width: 496, height: height) {
            MacAlbumCredits(credits: rows)
                .frame(width: 480, alignment: .leading)
        }
    }

    /// `MacAlbumCredits` with three credits and with one.
    private static let albumCreditsComponentScenes: [ShotScene] = [
        albumCredits(
            "mac.component.album-credits.three-credits",
            height: 160,
            [
                credit("Producer", "Ines Varga"),
                credit("Engineer", "Pell Marsh"),
                credit("Arranger", "Ada Lindqvist", tracks: ["2", "5"]),
            ]
        ),
        albumCredits(
            "mac.component.album-credits.one-credit",
            height: 110,
            [credit("Producer", "Ines Varga")]
        ),
    ]

    private static func workTrack(_ position: String, _ title: String)
        -> Track
    {
        var track = PreviewData.previewTrack(
            id: "track-\(position)",
            title: title,
            position: position
        )
        track.works = [
            BridgeTrackWork(
                workId: "work-\(position)",
                title: title,
                composerNames: "Ada Lindqvist"
            )
        ]
        return track
    }

    private static func albumWorks(
        _ id: String,
        height: CGFloat,
        _ tracks: [Track]
    ) -> ShotScene {
        component(id, width: 496, height: height) {
            MacAlbumWorks(tracks: tracks, onOpenWork: { _ in })
                .frame(width: 480, alignment: .leading)
        }
    }

    /// `MacAlbumWorks` with two works and with one.
    private static let albumWorksScenes: [ShotScene] = [
        albumWorks(
            "mac.component.album-works.two-works",
            height: 130,
            [
                workTrack("4", "Low Tide Suite: I. Ebb"),
                workTrack("9", "Harbor Waltz"),
            ]
        ),
        albumWorks(
            "mac.component.album-works.one-work",
            height: 100,
            [workTrack("4", "Low Tide Suite: I. Ebb")]
        ),
    ]

    private static func releasePicker(
        _ id: String,
        _ names: [String],
        shown: Int
    ) -> ShotScene {
        component(id, width: 300, height: 60) {
            MacReleasePicker(
                detail: panelDetail(releases: names, shown: shown),
                currentReleaseId: nil,
                onSelect: { _ in }
            )
            .frame(width: 284, alignment: .leading)
        }
    }

    /// `MacReleasePicker` over two releases and over three.
    private static let releasePickerScenes: [ShotScene] = [
        releasePicker(
            "mac.component.release-picker.two-releases",
            ["CD", "LP"],
            shown: 0
        ),
        releasePicker(
            "mac.component.release-picker.three-releases",
            ["CD", "LP", "Remaster"],
            shown: 1
        ),
    ]

    private static func albumHeader(
        _ id: String,
        _ detail: AlbumDetail,
        height: CGFloat = 250,
        optionsOpen: Bool = false
    ) -> ShotScene {
        component(id, width: 556, height: height) {
            MacAlbumHeader(
                detail: detail,
                currentReleaseId: nil,
                onSelectRelease: { _ in },
                onPlay: {},
                onRate: { _ in }
            ) {
                MacMenuItem("Edit…") {}
                MacMenuItem("Change cover…") {}
                MacMenuItem("Storage…") {}
                MacMenuItem.divider
                MacMenuItem("Delete album…") {}
            }
            .frame(width: 540, alignment: .leading)
            .overlay(alignment: .bottomLeading) {
                if optionsOpen {
                    // Under the ⋯, which follows Play above the rating line.
                    MacMenuInPlace(
                        {
                            MacMenuItem("Edit…") {}
                            MacMenuItem("Change cover…") {}
                            MacMenuItem("Storage…") {}
                            MacMenuItem.divider
                            MacMenuItem("Delete album…") {}
                        },
                        highlighted: 0
                    )
                    .alignmentGuide(.bottom) { $0[.top] + 22 }
                    .padding(.leading, 64)
                }
            }
        }
    }

    /// `MacAlbumHeader` at rest, over two releases, on another Mac's disk,
    /// rated, with its menu open and with no year.
    private static let albumHeaderScenes: [ShotScene] = [
        albumHeader("mac.component.album-header.resting", panelDetail()),
        albumHeader(
            "mac.component.album-header.two-releases",
            panelDetail(releases: ["CD", "LP"])
        ),
        albumHeader(
            "mac.component.album-header.unplayable",
            panelDetail(playability: onAnasMac)
        ),
        albumHeader(
            "mac.component.album-header.rated",
            panelDetail(rating: ratedByYouAndAna)
        ),
        albumHeader(
            "mac.component.album-header.options-open",
            panelDetail(),
            height: 340,
            optionsOpen: true
        ),
        albumHeader(
            "mac.component.album-header.no-year",
            panelDetail(year: nil)
        ),
    ]

    private static let albumPageScenes: [ShotScene] =
        albumRatingComponentScenes + albumCreditsComponentScenes
        + albumWorksScenes
        + releasePickerScenes + albumHeaderScenes

    // MARK: - The artist page's pieces

    /// `MacArtistAlbumGrid` over three albums, and `MacArtistOtherNames`
    /// with no other name yet.
    private static let artistPageScenes: [ShotScene] = [
        component(
            "mac.component.artist-album-grid.three-albums",
            width: 540,
            height: 250,
            images: coverStore()
        ) {
            MacArtistAlbumGrid(albums: [
                gridAlbum("Low Tide Hours", "Mara Quill", 1992),
                gridAlbum("Field Recordings", "Mara Quill", 1979),
                gridAlbum("Northbound", "Mara Quill", 1974),
            ])
            .frame(width: 524, alignment: .leading)
            .environment(UiStore())
        },
        component("mac.component.artist-other-names.empty", height: 90) {
            MacArtistOtherNames(artistId: "artist-mara-quill", names: [])
                .frame(width: 480, alignment: .leading)
                .environment(ArtistEditor())
                .environment(UiStore())
        },
    ]

    // MARK: - The sort and the library heading

    private static func sortPill(
        _ id: String,
        _ direction: BridgeSortDirection,
        removable: Bool = false
    ) -> ShotScene {
        component(id, height: 60) {
            MacSortPill(
                criterion: .constant(
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: direction
                    )
                ),
                takenFields: [],
                canRemove: removable,
                onSetField: { _ in },
                onRemove: {}
            )
        }
    }

    /// `MacSortPill` ascending, descending and removable, and
    /// `MacSortCriteria` with every field in use; `MacLibraryHeading` with
    /// its controls.
    private static let sortScenes: [ShotScene] = [
        sortPill("mac.component.sort-pill.up", .ascending),
        sortPill("mac.component.sort-pill.down", .descending),
        sortPill(
            "mac.component.sort-pill.removable",
            .descending,
            removable: true
        ),
    ]

    private static let libraryHeadingControlsScenes: [ShotScene] = [
        component(
            "mac.component.library-heading.controls",
            width: 596,
            height: 110
        ) {
            MacLibraryHeading(collapseProgress: 1, fullWidth: false) {
                MacSortCriteria(
                    criteria: .constant([
                        BridgeSortCriterion(
                            field: .dateAdded,
                            direction: .descending
                        )
                    ])
                )
            }
            .environment(UiStore())
        }
    ]

    private static func artistAlbumCard(
        _ title: String,
        _ year: Int32
    ) -> some View {
        MacArtistAlbumTile(
            album: gridAlbum(title, "Mara Quill", year),
            onTap: {}
        )
        .frame(width: 160)
    }

    static func albumTile(
        _ title: String,
        _ artist: String,
        _ year: Int32?,
        open: Bool = false,
        selected: Bool = false,
        covered: Bool = true,
        size: CGFloat = 160,
        menu: AlbumCardMenu
    ) -> some View {
        MacAlbumTile(
            title: title,
            artistNames: artist,
            year: year,
            cover: covered ? cover(title) : nil,
            isExpanded: open,
            isSelected: selected,
            size: size,
            menu: menu
        )
    }
}
