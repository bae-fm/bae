import BaeKit
import Foundation

/// What the Subsonic server is doing, as core last said: off, running at
/// which addresses, or not running and why. It is written by the value
/// subscription alone, and read by the Subsonic settings, the Settings
/// sidebar and the Activity panel.
@MainActor
@Observable
final class SubsonicStore {
    private(set) var view: BridgeSubsonicView

    /// Before core's first word, the server is taken to be off.
    init(
        view: BridgeSubsonicView = BridgeSubsonicView(
            status: .off,
            networks: [],
            passwordSaved: false
        )
    ) {
        self.view = view
    }

    func apply(_ view: BridgeSubsonicView) {
        guard self.view != view else { return }
        self.view = view
    }
}
