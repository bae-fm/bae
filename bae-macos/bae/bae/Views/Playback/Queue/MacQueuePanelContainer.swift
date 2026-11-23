import BaeKit
import SwiftUI

/// Reads the stores for `MacQueuePanel`: the tab and what starts the pane's
/// top come off the `UiStore`, and each tab's content reads its own.
struct MacQueuePanelContainer: View {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections

    let initialHoveredEntry: String?
    let onClose: () -> Void
    let onInsertTracks: ([String], Int) -> Void

    var body: some View {
        MacQueuePanel(
            tab: uiStore.sidePaneTab,
            onSelectTab: { uiStore.selectSidePaneTab($0) },
            lead: lead,
            onClose: onClose,
            content: {
                switch uiStore.sidePaneTab {
                case .queue:
                    QueueTabContainer(
                        initialHoveredEntry: initialHoveredEntry,
                        onInsertTracks: onInsertTracks
                    )
                case .playlists:
                    PlaylistsContent()
                case .history:
                    HistoryContent()
                }
            }
        )
    }

    /// On Playlists, the back button to the list while a playlist is open,
    /// else the + that makes a playlist, which a person with no profile
    /// cannot.
    private var lead: SidePaneLead {
        guard uiStore.sidePaneTab == .playlists else { return .none }
        if uiStore.openPlaylistId != nil {
            return .back(to: String(localized: "Playlists")) {
                uiStore.closePlaylist()
            }
        }
        guard libraryProjections.playlists.value?.canCreate == true else {
            return .none
        }
        return .newPlaylist {
            uiStore.presentNewPlaylist(pickedTrackIds: [])
        }
    }
}

/// Reads the stores for `QueueTabContent`: now-playing and the lanes come off
/// the `PlaybackStore`, and every command goes to `Queue` and `Playback`.
private struct QueueTabContainer: View {
    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var rowHover: QueueRowHover
    let onInsertTracks: ([String], Int) -> Void

    init(
        initialHoveredEntry: String?,
        onInsertTracks: @escaping ([String], Int) -> Void
    ) {
        _rowHover = State(
            initialValue: QueueRowHover(entryId: initialHoveredEntry)
        )
        self.onInsertTracks = onInsertTracks
    }

    var body: some View {
        let navigation = NowPlayingNavigationAction(
            playbackStore: playbackStore,
            uiStore: uiStore
        )
        let onGoToNowPlaying: (() -> Void)? =
            navigation.isEnabled ? { navigation.perform() } : nil
        let np = playbackStore.nowPlaying
        let library = np.display?.library
        let cover: ImageContent? =
            library?.coverImage
            .map {
                .libraryImage($0)
            }
        QueueTabContent(
            rowHover: rowHover,
            manual: playbackStore.manualQueue,
            upNextCount: playbackStore.upNextCount,
            isEmpty: playbackStore.queueIsEmpty,
            context: playbackStore.queueContext,
            shuffled: playbackStore.shuffled,
            upcoming: QueueUpcoming(store: playbackStore, queue: queue),
            nowPlaying: np.isActive
                ? QueueNowPlaying(
                    title: np.display?.title,
                    artist: library?.artistLine.text,
                    cover: cover,
                    position: playbackStore.trackPosition(forTrack: np.trackId)
                ) : nil,
            onShowUpcoming: { await playbackStore.showUpcoming(queue: queue) },
            isPlaying: np.isPlaying,
            onGoToNowPlaying: onGoToNowPlaying,
            onPlayPause: {
                playback.playPause(for: playbackStore.nowPlaying)
            },
            onClearUpNext: {
                Task {
                    do { try await queue.clearUpNext() }
                    catch { uiStore.showError(error) }
                }
            },
            onClearPlayingFrom: {
                Task {
                    do { try await queue.clearPlayingFrom() }
                    catch { uiStore.showError(error) }
                }
            },
            onSkipTo: { entryId in
                Task {
                    do { try await queue.skipToEntry(entryId) }
                    catch { uiStore.showError(error) }
                }
            },
            onRemove: { entryId in
                Task {
                    do { try await queue.removeEntry(entryId) }
                    catch { uiStore.showError(error) }
                }
            },
            onReorder: { entryId, lane, position in
                try await queue.reorderEntry(entryId, lane, position)
            },
            onReorderFailed: { uiStore.showError($0) },
            onInsertTracks: onInsertTracks,
            onDropFailed: { uiStore.showError($0) },
            onSetShuffle: { queue.setShuffle($0) },
        )
    }
}
