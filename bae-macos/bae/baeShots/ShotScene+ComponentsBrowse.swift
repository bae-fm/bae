import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the library's browse pages, the album panel,
    /// search, the queue's skeleton row and the playlist's page.
    static let browseComponentScenes: [ShotScene] =
        albumGridScenes + albumPanelScenes
        + artistGroupHeadingScenes + browseSplitScenes + detailHeaderScenes
        + lightboxScenes + thumbnailStripScenes + queuePlaceholderScenes
        + searchScenes + trackColumnsScenes + workDetailScenes
        + playlistPageScenes + playlistHeaderScenes + listRowScenes
        + savePanelFormatScenes

    /// A track as the album panel lists it.
    private static func browseTrack(
        _ id: String,
        _ title: String,
        position: String,
        minutes: UInt32,
        seconds: UInt32
    ) -> BridgeTrack {
        PreviewData.previewBridgeTrack(
            id: id,
            title: title,
            position: position,
            durationMs: UInt64(minutes * 60 + seconds) * 1000,
            artist: "Mara Quill"
        )
    }

    // MARK: - MacAlbumGrid, MacArtistGroupHeading

    static func gridAlbum(
        _ title: String,
        _ artist: String,
        _ year: Int32?
    ) -> BridgeAlbum {
        BridgeAlbum(
            id: title,
            title: title,
            year: year,
            isCompilation: false,
            artistLine: .named(artist),
            releaseIds: [],
            primaryReleaseId: title,
            cover: cover(title)
        )
    }

    private static func browseTile(
        _ title: String,
        _ artist: String,
        _ year: Int32?
    ) -> some View {
        albumTile(
            title,
            artist,
            year,
            size: 200,
            menu: AlbumCardMenu(
                targetCount: 1,
                onPlay: {},
                onAddToQueue: {},
                onAddNext: {},
                onDelete: {}
            )
        )
    }

    /// `MacAlbumGrid` over four albums, one row of four columns.
    private static let albumGridScenes: [ShotScene] = {
        let images = coverStore()
        return [
            component(
                "mac.component.album-grid.default",
                width: 4 * 200 + 3 * ThemeSpace.page
                    + 2 * LibraryContentContainer.horizontalPadding,
                height: 330,
                inset: 0,
                images: images
            ) {
                MacAlbumGrid(
                    list: AlbumList.preview(albums: [
                        gridAlbum("Low Tide Hours", "Mara Quill", 1992),
                        gridAlbum("Paper Lanterns", "The Slow Orchards", 1971),
                        gridAlbum("Glass Harbor", "Ines Varga Trio", 1959),
                        gridAlbum("Copper Sky", "Odile Fen", nil),
                    ]),
                    sortCriteria: [],
                    fullWidth: false,
                    selection: AlbumGridSelection(),
                    onPlay: { _ in },
                    onAddToQueue: { _ in },
                    onAddNext: { _ in },
                    onDeleteAlbum: { _ in }
                ) { _ in
                    EmptyView()
                }
                .albumDetailPreviewEnvironment()
                .environment(images)
            }
        ]
    }()

    /// `MacArtistGroupHeading` over the artist's two albums, as the grouped
    /// grid lays them out: the heading on its own line, the tiles under it.
    private static let artistGroupHeadingScenes: [ShotScene] = [
        component(
            "mac.component.artist-group-heading.default",
            width: 3 * 200 + 2 * ThemeSpace.page + 16,
            height: 200,
            images: coverStore()
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.page) {
                MacArtistGroupHeading(name: "Mara Quill")
                HStack(alignment: .top, spacing: ThemeSpace.page) {
                    browseTile("Low Tide Hours", "Mara Quill", 1992)
                    browseTile("Northbound", "Mara Quill", 1974)
                }
            }
        }
    ]

    // MARK: - MacAlbumPanel, MacTrackColumns

    /// The album the panel and the track list draw: "Low Tide Hours" on a
    /// release named `name`, side A with two tracks.
    private static func panelRelease(
        id: String,
        name: String,
        medium: BridgeMedium,
        covered: Bool,
        playability: BridgeReleasePlayability
    ) -> BridgeRelease {
        var release = PreviewData.boardAlbumDetail().releases[0]
        let facts = BridgePressingFacts(
            area: .country(code: "US"),
            media: PreviewData.media(medium),
            status: nil,
            packaging: nil,
            discogsDetails: []
        )
        release.id = id
        release.name = .named(name: name)
        release.facts = facts
        release.pressingSummary = bridgePressingSummary(facts: facts)
        release.mediaTerms = bridgeMediaTerms(media: facts.media)
        release.pressingDetails = bridgePressingDetails(facts: facts)
        release.labels = [
            BridgeLabelLine(
                names: ["Harbor Records"],
                catalogNumbers: []
            )
        ]
        release.cover = covered ? cover("Low Tide Hours") : nil
        release.playability = playability
        let tracks = [
            browseTrack(
                "a1",
                "Harbor Lights",
                position: "A1",
                minutes: 4,
                seconds: 32
            ),
            browseTrack(
                "a2",
                "The Long Way Home",
                position: "A2",
                minutes: 5,
                seconds: 12
            ),
        ]
        release.tracks = tracks
        release.trackGroups = [
            BridgeTrackGroup(
                side: .sided(sideLetter: "A"),
                tracks: tracks,
                totalDuration: nil,
                showsTime: false,
                secondColumnStart: nil
            )
        ]
        return release
    }

    /// "Low Tide Hours" with one release per name in `releases`, the one at
    /// `shown` showing; the first is a CD, the others LPs.
    static func panelDetail(
        releases names: [String] = ["CD"],
        shown: Int = 0,
        rating: BridgeAlbumRating = BridgeAlbumRating(mine: nil, raters: []),
        playability: BridgeReleasePlayability = .playable,
        year: Int32? = 1992,
        covered: Bool = true
    ) -> AlbumDetail {
        var detail = PreviewData.boardAlbumDetail()
        let releases = names.enumerated()
            .map { index, name in
                panelRelease(
                    id: "rel-\(index)",
                    name: name,
                    medium: index == 0 ? .cd : .vinyl,
                    covered: covered,
                    playability: playability
                )
            }
        detail.album.releaseIds = releases.map(\.id)
        detail.album.year = year
        detail.album.cover = covered ? cover("Low Tide Hours") : nil
        detail.releases = releases
        detail.rating = rating
        detail.showsReleaseChoices = releases.count > 1
        return AlbumDetail(
            bridge: detail.showing(chosenReleaseId: "rel-\(shown)")
        )
    }

    private static func albumPanel(
        _ id: String,
        detail: AlbumDetail
    ) -> ShotScene {
        component(
            id,
            width: 820,
            height: 480,
            images: browsePanelImages
        ) {
            MacAlbumPanel(
                detail: detail,
                onBrowseImages: {},
                onSelectRelease: { _ in },
                currentTrackId: "a1",
                currentReleaseId: nil,
                isPlaying: true,
                onClose: {},
                onPlay: {},
                onShuffle: {},
                onPlayFromTrack: { _ in },
                onTogglePlayPause: {},
                onAddNext: { _ in },
                onAddToQueue: { _ in },
                onAddNextAlbum: {},
                onAddAlbumToQueue: {},
                onEdit: {},
                onOpenStorage: {},
                onExportRelease: {},
                onSaveReleaseAs: {},
                onSetPrimaryRelease: {},
                onDeleteRelease: {},
                onDeleteAlbum: {},
                onExportTrack: { _ in },
                onOpenWork: { _ in },
                onRateAlbum: { _ in },
                onRateTrack: { _, _ in }
            )
            .albumDetailPreviewEnvironment()
            .environment(browsePanelImages)
        }
    }

    /// `MacAlbumPanel` for "Low Tide Hours", the first track playing: at
    /// rest, with a CD and an LP, rated by the person and by Ana, on another
    /// Mac's disk, and with no cover.
    private static let albumPanelScenes: [ShotScene] = [
        albumPanel("mac.component.album-panel.resting", detail: panelDetail()),
        albumPanel(
            "mac.component.album-panel.two-releases",
            detail: panelDetail(releases: ["CD", "LP"])
        ),
        albumPanel(
            "mac.component.album-panel.rated",
            detail: panelDetail(rating: ratedByYouAndAna)
        ),
        albumPanel(
            "mac.component.album-panel.unplayable",
            detail: panelDetail(playability: onAnasMac)
        ),
        albumPanel(
            "mac.component.album-panel.no-cover",
            detail: panelDetail(covered: false)
        ),
    ]

    /// Four stars from the person, and Ana's five.
    static let ratedByYouAndAna = BridgeAlbumRating(
        mine: .four,
        raters: [.you, .person(name: "Ana", stars: .five)]
    )

    /// A release whose audio is on Ana's Mac.
    static let onAnasMac = BridgeReleasePlayability.onOtherDevice(
        device: .active(name: "Ana’s Mac")
    )

    private static let browsePanelImages = coverStore()

    /// The release `MacTrackColumns` lists: two tracks, one in each column.
    private static func twoColumnRelease() -> ReleaseDetail {
        var release = PreviewData.boardAlbumDetail().releases[0]
        let tracks = [
            browseTrack(
                "t1",
                "Harbor Lights",
                position: "1",
                minutes: 4,
                seconds: 32
            ),
            browseTrack(
                "t6",
                "Low Tide Hours",
                position: "6",
                minutes: 5,
                seconds: 3
            ),
        ]
        release.tracks = tracks
        release.trackGroups = [
            BridgeTrackGroup(
                side: .flat,
                tracks: tracks,
                totalDuration: nil,
                showsTime: false,
                secondColumnStart: 1
            )
        ]
        return ReleaseDetail(bridge: release)
    }

    /// `MacTrackColumns`: two tracks, one in each column.
    private static let trackColumnsScenes: [ShotScene] = [
        component(
            "mac.component.track-columns.default",
            width: 596,
            height: 100
        ) {
            MacTrackColumns(
                release: twoColumnRelease(),
                isCompilation: false,
                currentTrackId: nil,
                isPlaying: false,
                onPlayFromTrack: { _ in },
                onTogglePlayPause: {},
                onAddNext: { _ in },
                onAddToQueue: { _ in },
                onExportTrack: { _ in },
                onRateTrack: { _, _ in }
            )
            .frame(width: 580, alignment: .leading)
            .environment(UiStore())
        }
    ]

    // MARK: - MacBrowseSplit, MacDetailHeader, MacWorkDetail

    private static func artistSummary(
        _ name: String,
        albums: UInt64
    ) -> BridgeArtistSummary {
        BridgeArtistSummary(
            artistId: name,
            name: name,
            countLine: BridgeCountLabel(
                key: "core.library.album_count",
                count: albums
            ),
            image: cover(name)
        )
    }

    /// `MacBrowseSplit`: two artists in the list, the first selected, and its
    /// header in the detail pane.
    private static let browseSplitScenes: [ShotScene] = [
        component(
            "mac.component.browse-split.default",
            width: 700,
            height: 360,
            inset: 0,
            images: coverStore()
        ) {
            MacBrowseSplit {
                VStack(spacing: 0) {
                    MacLibraryRow(
                        title: "Mara Quill",
                        subtitle: "12 albums",
                        selected: true
                    )
                    MacLibraryRow(title: "Odile Fen", subtitle: "3 albums")
                }
                .frame(maxHeight: .infinity, alignment: .top)
            } detail: {
                MacDetailHeader(
                    summary: artistSummary("Mara Quill", albums: 12)
                )
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(ThemeSpace.edge)
            }
        }
    ]

    /// `MacDetailHeader`: the artist's picture, name and count.
    private static let detailHeaderScenes: [ShotScene] = [
        component(
            "mac.component.detail-header.default",
            height: 100,
            images: coverStore()
        ) {
            MacDetailHeader(summary: artistSummary("Mara Quill", albums: 12))
                .frame(width: 380, alignment: .leading)
        }
    ]

    /// `MacWorkDetail`: a work with one release and one recording.
    private static let workDetailScenes: [ShotScene] = [
        component(
            "mac.component.work-detail.default",
            height: 260,
            images: coverStore()
        ) {
            let media = bridgeMediaTerms(media: PreviewData.media(.cd))
            MacWorkDetail(
                detail: BridgeWorkDetail(
                    work: BridgeWorkSummary(
                        workId: "work-suite",
                        title: "Low Tide Suite",
                        composerNames: nil,
                        linkedReleaseCount: 1,
                        representativeCover: nil
                    ),
                    childWorks: [],
                    releases: [
                        BridgeWorkReleaseSummary(
                            releaseId: "rel-blue-room",
                            albumId: "album-blue-room",
                            albumTitle: "Blue Room Sessions",
                            name: .described(year: nil, media: media),
                            media: media,
                            cover: cover("Blue Room Sessions")
                        )
                    ],
                    tracks: [
                        BridgeWorkTrackSummary(
                            trackId: "track-ebb",
                            trackTitle: "Low Tide Suite: I. Ebb",
                            releaseId: "rel-blue-room",
                            albumId: "album-blue-room",
                            albumTitle: "Blue Room Sessions"
                        )
                    ],
                    childWorkCount: 0,
                    releaseCount: 1,
                    trackCount: 1
                ),
                openWork: { _ in },
                openAlbum: { _, _ in }
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    // MARK: - MacLightbox, MacThumbnailStrip

    /// What a lightbox shows of its image.
    private enum LightboxImageState {
        case shown, failed
    }

    /// The four images the lightbox's strip lists.
    private static let lightboxImages = [
        "Low Tide Hours", "Low Tide Hours back", "Low Tide Hours disc",
        "Low Tide Hours booklet",
    ]

    private static func lightboxItems() -> [LightboxItem] {
        lightboxImages.map {
            LightboxItem(
                id: $0,
                label: "cover.jpg",
                image: .libraryImage(cover($0))
            )
        }
    }

    /// A store that gives the first image no bytes when it `failed`, and
    /// every image's art at once otherwise.
    private static func lightboxStore(_ state: LightboxImageState)
        -> ImageStore
    {
        let first = lightboxImages[0]
        return ImageStore(
            fetchLibraryImageBytes: { ref in
                if ref.id == first {
                    switch state {
                    case .shown:
                        return PreviewData.previewArtData(ref.id)
                    case .failed:
                        return nil
                    }
                }
                return PreviewData.previewArtData(ref.id)
            }
        )
    }

    private static func lightbox(
        _ id: String,
        state: LightboxImageState = .shown,
        browse: Bool = false
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 800, height: 640)
        ) {
            let images = lightboxStore(state)
            let load = LightboxImageLoad()
            let items = lightboxItems()
            let view = MacLightbox(
                place: LightboxPlace(
                    items: items,
                    current: items[0],
                    neighbors: LightboxNeighbors(
                        previous: items[3].id,
                        next: items[1].id
                    )
                ),
                load: load,
                onSelect: { _ in },
                onDismiss: {},
                onBrowseAll: browse ? {} : nil
            )
            .environment(images)
            switch state {
            case .shown:
                return ShotStage(view, images: images) {
                    load.phase.image != nil
                }
            case .failed:
                return ShotStage(view, images: images) {
                    if case .failed = load.phase { return true }
                    return false
                }
            }
        }
    }

    /// `MacLightbox` over the cover, with the thumbnail strip and the arrows:
    /// at rest, with Browse all images, and when the image could not load.
    private static let lightboxScenes: [ShotScene] = [
        lightbox("mac.component.lightbox.resting"),
        lightbox("mac.component.lightbox.browse", browse: true),
        lightbox("mac.component.lightbox.failed", state: .failed),
    ]

    private static func thumbnailStrip(
        _ id: String,
        current: Int
    ) -> ShotScene {
        component(id, width: 560, height: 90, inset: 0, images: coverStore()) {
            let items = lightboxItems()
            MacThumbnailStrip(
                items: items,
                current: items[current].id,
                centered: true,
                onSelect: { _ in },
                stroke: { _, isActive in
                    (
                        isActive ? Theme.onFill : Theme.onFillSecondary,
                        isActive ? 2 : 1
                    )
                }
            ) { item in
                Artwork(
                    content: item.image,
                    pointSize: MacThumbnailStripLayout.thumbnailSize
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.backdrop)
        }
    }

    /// `MacThumbnailStrip` of the lightbox's four images, on the first and on
    /// the second.
    private static let thumbnailStripScenes: [ShotScene] = [
        thumbnailStrip("mac.component.thumbnail-strip.first", current: 0),
        thumbnailStrip("mac.component.thumbnail-strip.second", current: 1),
    ]

    // MARK: - MacQueuePlaceholderRow

    /// `MacQueuePlaceholderRow`, the skeleton of an entry that is loading.
    private static let queuePlaceholderScenes: [ShotScene] = [
        component("mac.component.queue-placeholder-row.not-loaded", height: 80)
        {
            MacQueuePlaceholderRow()
                .frame(width: 380, alignment: .leading)
        }
    ]

    // MARK: - MacSearchCard, MacSearchSection

    /// `MacSearchSection` and `MacSearchCard` with an Albums and a Tracks
    /// section.
    private static let searchScenes: [ShotScene] = [
        component("mac.component.search-section.default", height: 60) {
            MacSearchSection("Albums")
                .frame(width: 380, alignment: .leading)
        },
        searchCard(
            "mac.component.search-card.results",
            height: 240,
            results: SearchResults(
                bridge: BridgeSearchResults(
                    albums: [
                        BridgeAlbumSearchResult(
                            id: "album-low-tide",
                            title: "Low Tide Hours",
                            year: 1992,
                            artistLine: .named("Mara Quill"),
                            cover: nil
                        )
                    ],
                    artists: [],
                    tracks: [
                        BridgeTrackSearchResult(
                            id: "track-harbor-lights",
                            title: "Harbor Lights",
                            durationClock: BridgeDurationClock(
                                negative: false,
                                hours: nil,
                                minutes: 4,
                                seconds: 32
                            ),
                            releaseId: "release-low-tide",
                            albumId: "album-low-tide",
                            albumTitle: "Low Tide Hours",
                            artistLine: .named("Mara Quill"),
                            cover: cover("Low Tide Hours")
                        )
                    ],
                    composers: [],
                    works: [],
                    found: true,
                    sectionCounts: BridgeSearchSectionCounts(
                        albums: 1,
                        artists: 0,
                        tracks: 1,
                        composers: 0,
                        works: 0
                    )
                ),
                query: "low tide"
            )
        ),
        searchCard(
            "mac.component.search-card.no-results",
            height: 270,
            results: SearchResults(
                bridge: BridgeSearchResults(
                    albums: [],
                    artists: [],
                    tracks: [],
                    composers: [],
                    works: [],
                    found: false,
                    sectionCounts: BridgeSearchSectionCounts(
                        albums: 0,
                        artists: 0,
                        tracks: 0,
                        composers: 0,
                        works: 0
                    )
                ),
                query: "harbr"
            )
        ),
    ]

    private static func searchCard(
        _ id: String,
        height: CGFloat,
        results: SearchResults
    ) -> ShotScene {
        component(id, width: 456, height: height, images: coverStore()) {
            MacSearchCard(
                results: results,
                onSelectAlbum: { _ in },
                onSelectTrack: { _ in },
                onSelectArtist: { _ in },
                onSelectComposer: { _ in },
                onSelectWork: { _ in }
            )
        }
    }

    // MARK: - MacPlaylistPage

    private static func playlistSummary(
        owner: String,
        sharing: BridgeSharing,
        yours: Bool
    ) -> BridgePlaylistSummary {
        BridgePlaylistSummary(
            id: "playlist-late-night",
            name: "Harbor evenings",
            sharing: sharing,
            owner: BridgePersonRef(
                person: "person-\(owner)",
                name: owner,
                initials: String(owner.prefix(1)),
                avatarVersion: nil
            ),
            yours: yours,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: 12
            ),
            totalDuration: .minutesOnly(minutes: 54),
            cover: .grid(
                covers: [
                    cover("Low Tide Hours"), cover("Paper Lanterns"),
                    cover("Glass Harbor"), cover("Copper Sky"),
                ]
            )
        )
    }

    private static func playlistPage(
        _ id: String,
        summary: BridgePlaylistSummary,
        optionsOpen: Bool = false
    ) -> ShotScene {
        component(id, width: 420, height: 240, images: coverStore()) {
            let backing = boardLibraryBacking(uiStore: UiStore())
            MacPlaylistPage(
                detail: BridgePlaylistDetail(
                    summary: summary,
                    entries: [],
                    empty: true
                )
            )
            .albumDetailPreviewEnvironment()
            .environment(backing.session)
            .frame(width: 404, alignment: .leading)
            .overlay(alignment: .topTrailing) {
                if optionsOpen {
                    // Under the ⋯ at the page's right: the page's margin and
                    // the picture's height down, the button's height further.
                    MacMenuInPlace {
                        PlaylistOptionsMenu(
                            onShuffle: {},
                            owner: PlaylistOptionsMenu.OwnerActions(
                                edit: {},
                                delete: {}
                            )
                        )
                    }
                    .padding(.top, ThemeSpace.section + 50 + 4)
                    .padding(.trailing, ThemeSpace.section)
                }
            }
        }
    }

    /// `MacPlaylistPage` for the person's own playlist shared with everyone
    /// and kept private, with its menu open, and for one another person
    /// shared.
    private static let playlistPageScenes: [ShotScene] = [
        playlistPage(
            "mac.component.playlist-page.only-me",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .private,
                yours: true
            )
        ),
        playlistPage(
            "mac.component.playlist-page.everyone",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            )
        ),
        playlistPage(
            "mac.component.playlist-page.no-options",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: false
            )
        ),
        playlistPage(
            "mac.component.playlist-page.options-open",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            ),
            optionsOpen: true
        ),
    ]

    // MARK: - MacPlaylistHeader

    private static func playlistHeader(
        _ id: String,
        summary: BridgePlaylistSummary,
        height: CGFloat = 110,
        optionsOpen: Bool = false
    ) -> ShotScene {
        component(id, width: 420, height: height, images: coverStore()) {
            MacPlaylistHeader(summary: summary, onEdit: {}, onDelete: {})
                .environment(Playback.stub())
                .frame(width: 404, alignment: .leading)
                .overlay(alignment: .topTrailing) {
                    if optionsOpen {
                        // Under the ⋯ at the header's right, which is centred
                        // beside the 72pt picture.
                        MacMenuInPlace {
                            PlaylistOptionsMenu(
                                onShuffle: {},
                                owner: PlaylistOptionsMenu.OwnerActions(
                                    edit: {},
                                    delete: {}
                                )
                            )
                        }
                        .padding(.top, 54)
                    }
                }
        }
    }

    /// `MacPlaylistHeader`: the person's own playlist shared with everyone
    /// and private, another person's, and the options menu open.
    private static let playlistHeaderScenes: [ShotScene] = [
        playlistHeader(
            "mac.component.playlist-header.only-me",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .private,
                yours: true
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.everyone",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.no-options",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: false
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.options-open",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            ),
            height: 230,
            optionsOpen: true
        ),
    ]

    // MARK: - MacListRow

    /// `MacListRow`, a playlist's track: at rest, with a person at its end,
    /// left faint and raised while the owner drags it, with a taller cover, and
    /// with its number hung in the margin.
    private static let listRowScenes: [ShotScene] = [
        listRow("mac.component.list-row.resting"),
        listRow("mac.component.list-row.who", who: true, wide: true),
        listRow("mac.component.list-row.lifted", lift: .lifted),
        listRow("mac.component.list-row.dragging", lift: .dragging),
        listRow("mac.component.list-row.cover", coverSize: 40),
        listRow(
            "mac.component.list-row.hanging",
            hanging: true,
            coverSize: 40,
            padsMargin: true
        ),
    ]

    private static func listRow(
        _ id: String,
        who: Bool = false,
        wide: Bool = false,
        lift: MacListRow.Lift = .resting,
        hanging: Bool = false,
        coverSize: CGFloat = 32,
        padsMargin: Bool = false
    ) -> ShotScene {
        component(id, height: 70, images: coverStore()) {
            MacListRow(
                lead: "21:04",
                leadWidth: 56,
                hanging: hanging,
                coverSize: coverSize,
                cover: cover("Low Tide Hours"),
                title: "Harbor Lights",
                subtitle: "Mara Quill · Low Tide Hours",
                who: who
                    ? BridgePersonRef(
                        person: "person-ana",
                        name: "Ana",
                        initials: "A",
                        avatarVersion: nil
                    ) : nil,
                duration: "4:32",
                wide: wide,
                lift: lift
            )
            .padding(
                .leading,
                padsMargin ? MacListRow.hang(leadWidth: 56) : 0
            )
            .frame(width: 380, alignment: .leading)
        }
    }

    // MARK: - MacSavePanelFormat

    /// The Save as panel's format line with `formats` in its pop-up.
    private struct SavePanelFormatShot: NSViewRepresentable {
        let formats: [String]

        func makeNSView(context _: Context) -> NSView {
            let popup = NSPopUpButton(
                frame: MacSavePanelFormat.popupFrame,
                pullsDown: false
            )
            popup.addItems(withTitles: formats)
            return MacSavePanelFormat(popup: popup)
        }

        func updateNSView(_: NSView, context _: Context) {}
    }

    /// `MacSavePanelFormat` closed, on FLAC.
    private static let savePanelFormatScenes: [ShotScene] = [
        component("mac.component.save-panel-format.closed", height: 50) {
            SavePanelFormatShot(formats: ["FLAC", "MP3", "Phone"])
                .frame(width: 250, height: 34)
        }
    ]
}
