import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Add Discogs Key... on the empty library and the empty Import tab: offered
/// while core says Discogs can't be used, and landing on the Import pane of
/// Settings, where the key is entered.
///
/// Whether it is offered is the `library-empty-no-key` and
/// `import-empty-no-key` shots': a SwiftUI button leaves no AppKit view a
/// test could count but a private SwiftUI key-view stand-in.
@MainActor
@Suite("Add Discogs Key on the empty states")
struct AddDiscogsKeyButtonTests {
    /// How many times Settings was asked for, and on which pane.
    @MainActor
    private final class SettingsRequests {
        var count = 0
        var tabWhenOpened: SettingsTab?
    }

    /// The button runs `openKeySettings`; where it sits is no longer tested.
    @Test("pressing it opens Settings on the Import pane")
    func pressingItOpensImportSettings() {
        let navigation = SettingsNavigation()
        let requests = SettingsRequests()
        AddDiscogsKeyButton.openKeySettings(
            navigation: navigation,
            openSettings: OpenSettingsWindowAction {
                requests.count += 1
                requests.tabWhenOpened = navigation.selectedTab
            }
        )
        #expect(requests.count == 1)
        #expect(requests.tabWhenOpened == .importing)
    }
}
