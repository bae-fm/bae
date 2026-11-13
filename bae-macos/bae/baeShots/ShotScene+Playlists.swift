import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Playlists view with the first playlist open, as the board draws
    /// it.
    static let playlistScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playlists.list",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            uiStore.setLibraryBrowserMode(.playlists)
            let backing = boardLibraryBacking(
                uiStore: uiStore,
                playlists: boardPlaylistListing,
                playlistDetails: boardPlaylistDetails
            )
            backing.session.selectPlaylist("playlist-late-night")
            let projections = LibraryProjectionStore(library: backing.library)
            return ShotStage(
                PreviewScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: (backing.library, backing.session),
                    playbackStore: boardPlayback(),
                    projections: projections,
                    size: CGSize(width: 1200, height: 760)
                )
            ) {
                projections.playlist.delivered
            }
        }
    ]

    private static func owner(_ name: String) -> BridgePlaylistOwner {
        BridgePlaylistOwner(
            person: "person-\(name)",
            name: name,
            avatarVersion: nil
        )
    }

    private static func summary(
        _ name: String,
        tracks: UInt32,
        sharing: BridgeSharing,
        minutes: UInt64
    ) -> BridgePlaylistSummary {
        BridgePlaylistSummary(
            id: "playlist-"
                + name.lowercased().replacingOccurrences(of: " ", with: "-"),
            name: name,
            sharing: sharing,
            owner: owner("Mara Quill"),
            yours: true,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: tracks
            ),
            totalDuration: .minutesOnly(minutes: minutes),
            cover: .single(cover: nil)
        )
    }

    nonisolated(unsafe) static let boardPlaylistListing:
        [BridgePlaylistSummary] = [
            summary("Late night", tracks: 12, sharing: .household, minutes: 54),
            summary(
                "Kitchen radio",
                tracks: 40,
                sharing: .household,
                minutes: 168
            ),
            summary("Running", tracks: 18, sharing: .private, minutes: 72),
            summary("For Ana", tracks: 9, sharing: .private, minutes: 38),
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
    ]

    nonisolated(unsafe) static let boardPlaylistDetails:
        [String: BridgePlaylistDetail] = [
            "playlist-late-night": BridgePlaylistDetail(
                summary: boardPlaylistListing[0],
                entries: boardEntries.enumerated()
                    .map { index, entry in
                        BridgePlaylistEntryRow(
                            positionId: "position-\(index)",
                            trackId: "track-\(index)",
                            title: entry.title,
                            artistNames: entry.artists,
                            albumId: "album-\(index)",
                            albumTitle: entry.album,
                            releaseId: "release-\(index)",
                            durationClock: entry.clock,
                            cover: nil
                        )
                    }
            )
        ]
}
