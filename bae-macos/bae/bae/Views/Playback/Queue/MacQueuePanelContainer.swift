import BaeKit
import SwiftUI

/// Reads the stores for `MacQueuePanel`: now-playing and the lanes come off the
/// `PlaybackStore`, and every command goes to `Queue` and `Playback`.
struct MacQueuePanelContainer: View {
    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    @Environment(UiStore.self)
    private var uiStore

    let onClose: () -> Void
    let onInsertTracks: ([String], Int) -> Void

    var body: some View {
        let navigation = NowPlayingNavigationAction(
            playbackStore: playbackStore,
            uiStore: uiStore
        )
        let onGoToNowPlaying: (() -> Void)? =
            navigation.isEnabled ? { navigation.perform() } : nil
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.display.coverImage
            .map {
                .libraryImage($0)
            }
        MacQueuePanel(
            manual: playbackStore.manualQueue,
            manualVersion: playbackStore.manualVersion,
            context: playbackStore.queueContext,
            upcoming: QueueUpcoming(store: playbackStore, queue: queue),
            position: playbackStore.trackPosition(
                forTrack: track?.track.trackId
            ),
            onShowUpcoming: { playbackStore.showUpcoming(queue: queue) },
            isActive: np.isActive,
            nowPlayingTitle: track?.display.title,
            nowPlayingArtist: track?.display.artistNames,
            nowPlayingCover: cover,
            isPlaying: np.isPlaying,
            onClose: onClose,
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
            onReorder: { entryId, beforeEntryId in
                try await queue.reorderEntry(entryId, beforeEntryId)
            },
            onReorderFailed: { uiStore.showError($0) },
            onInsertTracks: onInsertTracks,
            onSetShuffle: { queue.setShuffle($0) },
        )
    }
}

#if DEBUG
    // MARK: - Previews

    /// Previews render the panel — the queue's root, chrome included — with the
    /// same environment wiring production uses: now-playing and the lanes come
    /// off the `PlaybackStore`, and commands go to the echoing preview `Queue`,
    /// whose snapshot re-application stands in for core's queue value delivery —
    /// without it, a committed drag's display order never reconciles and the
    /// next drag misbehaves.
    @MainActor
    private func queuePanelPreview(
        _ fixture: QueuePreviewFixture
    ) -> some View {
        MacQueuePanelContainer(
            onClose: {},
            onInsertTracks: { ids, index in
                fixture.queue.insertInQueue(ids, UInt32(index))
            }
        )
        .environment(Playback.stub())
        .environment(fixture.store)
        .environment(fixture.queue)
        .environment(ImageStore.stub())
        .environment(UiStore())
        // The preview canvas proposes ideal size, under which the panel's
        // ScrollView collapses; a firm height stands in for the window slot
        // the sidebar fills in the app.
        .frame(height: 720)
        .background(Theme.background)
    }

    #Preview("With items") {
        let fixture = PreviewData.echoingQueue(
            manualCount: 2,
            shuffled: true
        )
        fixture.store.play(
            track: BridgeNowPlayingTrack(
                track: BridgePlayingTrack(
                    trackId: "t-np",
                    durationMs: 214_000
                ),
                display: BridgeTrackDisplay(
                    title: PreviewData.nowPlayingTitle,
                    artistNames: PreviewData.nowPlayingArtist,
                    albumId: "a-01",
                    releaseId: "r-01",
                    albumTitle: "Album Title",
                    coverImage: nil
                )
            )
        )
        return queuePanelPreview(fixture)
    }

    #Preview("Empty") {
        queuePanelPreview(
            PreviewData.echoingQueue(manualCount: 0, context: false)
        )
    }
#endif
