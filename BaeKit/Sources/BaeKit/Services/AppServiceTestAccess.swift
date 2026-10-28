#if DEBUG
    import Combine

    @MainActor
    final class AppServiceTestAccess {
        private let playbackStore: PlaybackStore

        init(playbackStore: PlaybackStore) {
            self.playbackStore = playbackStore
        }

        var state: AppServiceTestState {
            AppServiceTestState(
                nowPlaying: playbackStore.nowPlaying,
                manualQueueEntryIds: playbackStore.manualQueue.map(\.entryId),
                volume: playbackStore.volume,
                isMuted: playbackStore.isMuted,
                repeatMode: playbackStore.repeatMode
            )
        }

        var queueItemsAddedPublisher: AnyPublisher<Int, Never> {
            playbackStore.queueItemsAddedPublisher
        }
    }
#endif
