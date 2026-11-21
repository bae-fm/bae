import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTitleBar` over the library, 1100pt wide as the main window's
    /// preview draws it.
    static let macTitleBarScenes: [ShotScene] = [
        titleBar("mac.component.title-bar.library", section: .library),
        titleBar("mac.component.title-bar.import", section: .importing),
        titleBar(
            "mac.component.title-bar.moving",
            section: .library,
            activity: movingActivity
        ),
        titleBar(
            "mac.component.title-bar.attention",
            section: .library,
            activity: attentionActivity
        ),
        titleBar(
            "mac.component.title-bar.paused",
            section: .library,
            activity: pausedActivity
        ),
        titleBar(
            "mac.component.title-bar.searching",
            section: .library,
            search: "harbor"
        ),
        titleBar("mac.component.title-bar.picture", section: .library),
    ]

    /// A store whose main window shows `section`.
    @MainActor
    static func storeShowing(_ section: MainSection) -> UiStore {
        let store = UiStore()
        store.activeSection = section
        return store
    }

    private static func titleBar(
        _ id: String,
        section: MainSection,
        activity: BridgeSyncActivity? = nil,
        search: String = ""
    ) -> ShotScene {
        component(id, width: 1_100, height: 56) {
            let uiStore = storeShowing(section)
            let backing = boardLibraryBacking(uiStore: uiStore)
            MacTitleBar(
                searchText: .constant(search),
                onSearchFieldFrame: { _ in }
            )
            .environment(uiStore)
            .environment(LibraryProjectionStore(library: backing.library))
            .environment(activityStore(activity))
            .environment(SubsonicStore())
            .environment(SettingsNavigation())
            // Ana, whose picture ends the bar.
            .environment(
                MembershipStore.stub(
                    people: PreviewData.members(
                        reaching: PreviewData.previewDropboxStorage
                    ),
                    selfIsAdmin: true
                )
            )
            .environment(PreviewData.syncStatus(hasCloudHome: true))
        }
    }
}
