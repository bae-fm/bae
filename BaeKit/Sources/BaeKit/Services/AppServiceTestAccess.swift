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
                settings: playbackStore.settings
            )
        }

        var queueItemsAddedPublisher: AnyPublisher<Int, Never> {
            playbackStore.queueItemsAddedPublisher
        }
    }
#endif
