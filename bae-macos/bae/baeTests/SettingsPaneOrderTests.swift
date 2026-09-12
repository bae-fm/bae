import Foundation
import Testing

@testable import bae

/// The sidebar lists the panes in `allCases` order, so the order the cases are
/// declared in is the order a person reads down the window: what the app looks
/// like, the library it holds, playing from it, getting releases in and out,
/// the servers and automation around it, then the app itself.
@MainActor
@Suite("The settings panes")
struct SettingsPaneOrderTests {
    @Test("the sidebar reads in the order the panes are declared")
    func theSidebarReadsInDeclaredOrder() {
        #expect(
            SettingsTab.allCases == [
                .appearance,
                .library,
                .playback,
                .importing,
                .formats,
                .transfers,
                .casting,
                .subsonic,
                .automation,
                .about,
            ]
        )
    }

    /// Opening settings with no pane named lands on the library, not on
    /// whichever pane happens to be declared first.
    @Test("settings open on the library")
    func settingsOpenOnTheLibrary() {
        #expect(SettingsNavigation().selectedTab == .library)
    }
}
