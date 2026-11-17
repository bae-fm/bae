#if DEBUG
    @MainActor
    final class AppServiceTestAccess {
        private let playbackStore: PlaybackStore

        init(playbackStore: PlaybackStore) {
            self.playbackStore = playbackStore
        }

        var state: AppServiceTestState {
            AppServiceTestState(
                nowPlaying: playbackStore.nowPlaying,
                failure: playbackStore.failure
            )
        }
    }
#endif
