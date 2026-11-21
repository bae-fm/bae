import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueuePanel`, one shot for each row of its preview.
    static let macQueuePanelScenes: [ShotScene] = [
        // `MacQueuePanel`, docked at the window's right: with a track playing,
        // paused, with a track that has no cover, and with an empty queue.
        queuePanel(
            "mac.component.queue-panel.playing",
            QueuePlaying(isPlaying: true, covered: true)
        ),
        queuePanel(
            "mac.component.queue-panel.paused",
            QueuePlaying(isPlaying: false, covered: true)
        ),
        queuePanel(
            "mac.component.queue-panel.no-cover",
            QueuePlaying(isPlaying: true, covered: false)
        ),
        queuePanel("mac.component.queue-panel.empty", nil),
        sidePanel(
            "mac.component.queue-panel.playlists",
            tab: .playlists,
            lead: .newPlaylist {}
        ) {
            PlaylistList(listing: panelPlaylists)
        },
        sidePanel(
            "mac.component.queue-panel.playlist-open",
            tab: .playlists,
            lead: .back(to: String(localized: "Playlists")) {}
        ) {
            Color.clear
        },
        sidePanel("mac.component.queue-panel.history", tab: .history) {
            MacQueueSection(
                title: String(localized: "Today"),
                kind: .day(
                    menu: historyMenu(person: nil)
                        .inlineMenu(action: false) { _ in }
                )
            )
            // The margin the plays' times hang into, which the preview
            // leaves 40pt wide.
            .padding(
                .leading,
                ThemeSpace.related + MacListRow.hang(leadWidth: 40)
            )
            .padding(.trailing, ThemeSpace.related)
        },
        sidePanel(
            "mac.component.queue-panel.history-empty",
            tab: .history
        ) {
            HistoryEmpty(menu: historyMenu(person: nil)) { _ in }
        },
        sidePanel(
            "mac.component.queue-panel.history-empty-for-a-person",
            tab: .history
        ) {
            HistoryEmpty(menu: historyMenu(person: benMemberId)) { _ in }
        },
    ]

    /// One of Mara Quill's tracks in the queue, as the boards list it, with
    /// its album's cover unless `covered` is false.
    static func queueEntry(
        _ title: String,
        album: String,
        duration: UInt64,
        covered: Bool = true
    ) -> BridgeQueueEntry {
        BridgeQueueEntry(
            entryId: "e-\(title)",
            trackId: "t-\(title)",
            display: BridgeTrackDisplay(
                title: title,
                artistLine: .named("Mara Quill"),
                albumId: "a-\(album)",
                releaseId: "r-\(album)",
                albumTitle: album,
                coverImage: covered ? cover(album) : nil
            ),
            durationClock: BridgeDurationClock.length(ms: duration)
        )
    }

    /// A queue with a track playing: the lanes it holds, whether the track
    /// plays and whether it has a cover.
    @MainActor
    private struct QueuePlaying {
        let upcoming: [BridgeQueueEntry]
        let manual: [BridgeQueueEntry]
        let isPlaying: Bool
        let covered: Bool

        init(isPlaying: Bool, covered: Bool) {
            upcoming = [
                ShotScene.queueEntry(
                    "Tin Roof",
                    album: "Low Tide Hours",
                    duration: 341_000
                )
            ]
            manual = [
                ShotScene.queueEntry(
                    "Margins",
                    album: "Northbound",
                    duration: 295_000
                )
            ]
            self.isPlaying = isPlaying
            self.covered = covered
        }
    }

    /// The queue panel over what plays, or empty with `nil`.
    private static func queuePanel(
        _ id: String,
        _ queue: QueuePlaying?
    ) -> ShotScene {
        let upcoming = queue?.upcoming ?? []
        let manual = queue?.manual ?? []
        let isPlaying = queue?.isPlaying ?? false
        let isActive = queue != nil
        return component(
            id,
            width: 420,
            height: 616,
            images: coverStore()
        ) {
            MacQueuePanel(tab: .queue, onSelectTab: { _ in }, onClose: {}) {
                QueueTabContent(
                    manual: manual,
                    upNextCount: UInt32(manual.count),
                    isEmpty: !isActive,
                    context: isActive
                        ? QueuePlaybackContext(
                            bridge: BridgePlaybackContext(
                                kind: .release,
                                sourceTitle: "Low Tide Hours"
                            )
                        ) : nil,
                    shuffled: false,
                    upcoming: QueueUpcoming(
                        total: upcoming.count,
                        itemAt: {
                            upcoming.indices.contains($0) ? upcoming[$0] : nil
                        },
                        epoch: nil,
                        holdRow: { _ in },
                        failure: nil,
                        onRetry: {}
                    ),
                    nowPlaying: queue.map(queueNowPlaying),
                    onShowUpcoming: {},
                    isPlaying: isPlaying,
                    onGoToNowPlaying: {},
                    onPlayPause: {},
                    onClearUpNext: {},
                    onClearPlayingFrom: {},
                    onSkipTo: { _ in },
                    onRemove: { _ in },
                    onReorder: { _, _, _ in true },
                    onReorderFailed: { _ in },
                    onInsertTracks: { _, _ in },
                    onDropFailed: { _ in },
                    onSetShuffle: { _ in }
                )
            }
        }
    }

    /// The track `playing` names, as the queue panel's top shows it.
    private static func queueNowPlaying(_ playing: QueuePlaying)
        -> QueueNowPlaying
    {
        QueueNowPlaying(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            cover: playing.covered
                ? .libraryImage(cover("Low Tide Hours")) : nil,
            position: PreviewData.trackPosition(
                positionMs: 102_000,
                durationMs: 303_000
            )
        )
    }

    /// The playlists the panel's preview lists: Ana's and one Ben shared.
    private static var panelPlaylists: BridgePlaylistListing {
        let playlists = [
            boardPlaylistListing[0],
            BridgePlaylistSummary(
                id: "playlist-sunday-mornings",
                name: "Sunday mornings",
                sharing: .household,
                owner: owner("Ben"),
                yours: false,
                trackCount: BridgeCountLabel(
                    key: "core.library.track_count",
                    count: 1
                ),
                totalDuration: .minutesOnly(minutes: 4),
                cover: .single(cover: cover("Copper Sky"))
            ),
        ]
        return playlistListing(playlists)
    }

    /// History's choice of whose plays, offering Ana and Ben, on the plays
    /// of the person with member id `person`, or everyone's for `nil`.
    private static func historyMenu(person: String?) -> HistoryMenu {
        HistoryMenu(
            count: BridgeListCount(
                rows: 0,
                items: 0,
                extra: boardHistoryMenu(
                    [(anaMemberId, "Ana"), (benMemberId, "Ben")],
                    person: person
                )
            )
        )
    }

    /// The panel on `tab`, starting its top with `lead`, holding `content`,
    /// in the preview's 600pt-tall frame.
    private static func sidePanel<Content: View>(
        _ id: String,
        tab: SidePaneTab,
        lead: SidePaneLead = .none,
        @ViewBuilder content: @escaping @MainActor () -> Content
    ) -> ShotScene {
        component(id, width: 420, height: 616, images: coverStore()) {
            // The tabs' content reads the library, the window's state and
            // the grid's order for what a click or a drop does.
            let uiStore = UiStore()
            let backing = boardLibraryBacking(uiStore: uiStore)
            MacQueuePanel(
                tab: tab,
                onSelectTab: { _ in },
                lead: lead,
                onClose: {},
                content: { content() }
            )
            .frame(height: 600)
            .environment(backing.library)
            .environment(backing.session)
            .environment(uiStore)
        }
    }
}
