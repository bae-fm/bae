import Combine
import Observation
import Sparkle

/// Whether Sparkle can check for updates now, and the check itself, for the
/// app menu's and the About pane's "Check for Updates" items.
@MainActor
@Observable
final class CheckForUpdatesViewModel {
    private(set) var canCheckForUpdates = false

    @ObservationIgnored
    private let updater: SPUUpdater
    @ObservationIgnored
    private var canCheckSubscription: AnyCancellable?

    init(updater: SPUUpdater) {
        self.updater = updater
        canCheckSubscription = updater.publisher(for: \.canCheckForUpdates)
            .sink { [weak self] canCheck in
                self?.canCheckForUpdates = canCheck
            }
    }

    func checkForUpdates() {
        updater.checkForUpdates()
    }
}
