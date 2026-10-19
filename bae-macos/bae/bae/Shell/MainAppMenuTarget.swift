import BaeKit

/// The open library's services that the main menu's library commands act
/// through and are enabled from: the same service and store instances
/// installed in the primary window's views.
@MainActor
final class MainAppMenuTarget {
    let playbackStore: PlaybackStore
    let configStore: ConfigStore
    let libraryStore: LibraryStore
    let uiStore: UiStore
    let library: Library
    let playback: Playback
    let membershipStore: MembershipStore

    init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        libraryStore: LibraryStore,
        uiStore: UiStore,
        library: Library,
        playback: Playback,
        membershipStore: MembershipStore
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.libraryStore = libraryStore
        self.uiStore = uiStore
        self.library = library
        self.playback = playback
        self.membershipStore = membershipStore
    }
}
