import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The side pane's Playlists tab, listing the playlists or with the
    /// first one open, as the boards draw it, and the menus, sheets and
    /// alert its page opens.
    static let playlistScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playback.side-playlists",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow(open: false)
        },
        ShotScene(
            id: "mac.playlists.list",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow()
        },
        newPlaylistScene("mac.playlists.new-playlist", sharing: .private),
        newPlaylistScene(
            "mac.playlists.new-playlist.everyone",
            sharing: .household
        ),
        ShotScene(
            id: "mac.playlists.options",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow { window in
                AnyView(
                    ZStack(alignment: .topLeading) {
                        window
                        MacMenuInPlace(
                            {
                                PlaylistOptionsMenu(
                                    onShuffle: {},
                                    owner: PlaylistOptionsMenu.OwnerActions(
                                        edit: {},
                                        delete: {}
                                    )
                                )
                            },
                            highlighted: 1
                        )
                        .padding(.leading, 995)
                        .padding(.top, 230)
                    }
                )
            }
        },
        ShotScene(
            id: "mac.playlists.edit",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow { window in
                AnyView(
                    OverWindow.sheet(
                        EditPlaylistSheet(
                            state: .constant(
                                EditPlaylistSheetState(boardPlaylistListing[0])
                            ),
                            onCancel: {},
                            onSave: { _ in }
                        ),
                        over: window
                    )
                )
            }
        },
        ShotScene(
            id: "mac.playlists.delete",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow { window in
                AnyView(
                    OverWindow.alert(
                        DeletePlaylistConfirmation.alert(
                            name: boardPlaylistListing[0].name
                        ) {},
                        over: window
                    )
                )
            }
        },
        ShotScene(
            id: "mac.playlists.track-menu",
            size: CGSize(width: 1200, height: 760)
        ) {
            playlistsWindow { window in
                AnyView(
                    ZStack(alignment: .topLeading) {
                        window
                        MacMenuInPlace(
                            {
                                PlaylistEntryMenu(
                                    play: {},
                                    playNext: {},
                                    addToQueue: {},
                                    remove: {}
                                )
                            },
                            highlighted: 4
                        )
                        .padding(.leading, 900)
                        .padding(.top, 327)
                    }
                )
            }
        },
    ]

    /// The New playlist sheet over the side pane's playlists, its name typed
    /// and `sharing` chosen.
    private static func newPlaylistScene(
        _ id: String,
        sharing: BridgeSharing
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 1200, height: 760)) {
            playlistsWindow(open: false) { window in
                AnyView(
                    OverWindow.sheet(
                        NewPlaylistSheet(
                            state: .constant(
                                NewPlaylistSheetState(
                                    name: "Sunday morning",
                                    sharing: sharing,
                                    pickedTrackIds: []
                                )
                            ),
                            onCancel: {},
                            onCreate: { _ in }
                        ),
                        over: window
                    )
                )
            }
        }
    }

    /// The window with the side pane on Playlists, the first playlist open
    /// when `open` is set, `dressed` as the scene needs: an overlay over
    /// it, or as it is.
    private static func playlistsWindow(
        open: Bool = true,
        dressed: (AnyView) -> AnyView = { $0 }
    ) -> ShotStage {
        let uiStore = UiStore()
        uiStore.openSidePane(.playlists)
        if open {
            uiStore.openPlaylist("playlist-late-night")
        }
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            albumCount: 6,
            playlists: boardPlaylistListing,
            playlistDetails: boardPlaylistDetails
        )
        let projections = LibraryProjectionStore(library: backing.library)
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: (backing.library, backing.session),
            playbackStore: boardPlayback(),
            projections: projections,
            size: CGSize(width: 1200, height: 760)
        )
        return ShotStage(dressed(AnyView(window))) {
            open
                ? projections.playlist.delivered
                : projections.playlists.delivered
        }
    }

    static func owner(_ name: String) -> BridgePersonRef {
        BridgePersonRef(
            person: "person-\(name)",
            name: name,
            initials: String(name.prefix(1)),
            avatarVersion: nil
        )
    }

    private static func summary(
        _ name: String,
        tracks: UInt64,
        sharing: BridgeSharing,
        minutes: UInt64,
        cover: BridgePlaylistCover
    ) -> BridgePlaylistSummary {
        BridgePlaylistSummary(
            id: "playlist-"
                + name.lowercased().replacingOccurrences(of: " ", with: "-"),
            name: name,
            sharing: sharing,
            owner: owner("Ana"),
            yours: true,
            headerSharing: sharing,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: tracks
            ),
            totalDuration: .minutesOnly(minutes: minutes),
            cover: cover
        )
    }

    /// A playlist on four different releases, none of which has a cover.
    private static let fourReleases = BridgePlaylistCover.grid(
        covers: [nil, nil, nil, nil]
    )

    static let boardPlaylistListing: [BridgePlaylistSummary] = [
        summary(
            "Late night",
            tracks: 12,
            sharing: .household,
            minutes: 54,
            cover: fourReleases
        ),
        summary(
            "Kitchen radio",
            tracks: 40,
            sharing: .household,
            minutes: 168,
            cover: fourReleases
        ),
        summary(
            "Running",
            tracks: 18,
            sharing: .private,
            minutes: 72,
            cover: .single(cover: nil)
        ),
        summary(
            "For Ana",
            tracks: 9,
            sharing: .private,
            minutes: 38,
            cover: .single(cover: nil)
        ),
    ]

    /// A track of the board's playlist: its title, artists, album and
    /// length.
    private struct BoardEntry {
        let title: String
        let artists: String
        let album: String
        let clock: BridgeDurationClock

        init(
            _ title: String,
            _ artists: String,
            _ album: String,
            _ minutes: UInt32,
            _ seconds: UInt32
        ) {
            self.title = title
            self.artists = artists
            self.album = album
            clock = BridgeDurationClock(
                negative: false,
                hours: nil,
                minutes: minutes,
                seconds: seconds
            )
        }
    }

    private static let boardEntries: [BoardEntry] = [
        BoardEntry("Harbor Lights", "Mara Quill", "Low Tide Hours", 4, 32),
        BoardEntry("Copper Sky", "Odile Fen", "Copper Sky", 3, 58),
        BoardEntry("Night Ferry", "Mara Quill", "Low Tide Hours", 4, 36),
        BoardEntry("Small Hours", "Tomas Brell", "Small Hours", 6, 10),
        BoardEntry(
            "Paper Lanterns",
            "The Slow Orchards",
            "Paper Lanterns",
            5,
            1
        ),
        BoardEntry("Slow Morning", "Mara Quill", "Low Tide Hours", 10, 22),
        BoardEntry("Harbor Wall", "Ines Varga Trio", "Glass Harbor", 3, 12),
        BoardEntry(
            "Lantern Walk",
            "The Slow Orchards",
            "Paper Lanterns",
            2,
            58
        ),
        BoardEntry("Copper Wire", "Odile Fen", "Copper Sky", 3, 40),
        BoardEntry("Last Ferry Home", "Tomas Brell", "Small Hours", 3, 5),
        BoardEntry("Salt Glass", "Ines Varga Trio", "Glass Harbor", 3, 31),
        BoardEntry("Morning Tide", "Mara Quill", "Low Tide Hours", 3, 15),
    ]

    static let boardPlaylistDetails: [String: BridgePlaylistDetail] = [
        "playlist-late-night": BridgePlaylistDetail(
            summary: boardPlaylistListing[0],
            entries: boardEntries.enumerated()
                .map { index, entry in
                    BridgePlaylistEntryRow(
                        positionId: "position-\(index)",
                        trackId: "track-\(index)",
                        title: entry.title,
                        artistLine: .named(entry.artists),
                        albumId: "album-\(index)",
                        albumTitle: entry.album,
                        releaseId: "release-\(index)",
                        durationClock: entry.clock,
                        cover: nil,
                        number: UInt64(index + 1),
                        canMoveUp: index > 0,
                        canMoveDown: index < boardEntries.count - 1
                    )
                },
            empty: false
        )
    ]
}
