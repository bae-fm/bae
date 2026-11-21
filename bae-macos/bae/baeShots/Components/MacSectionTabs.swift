import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSectionTabs` on each section.
    static let macSectionTabsScenes: [ShotScene] = [
        component("mac.component.section-tabs.library", width: 224, height: 60)
        {
            MacSectionTabs(selection: .library, onSelect: { _ in })
        },
        component("mac.component.section-tabs.import", width: 224, height: 60) {
            MacSectionTabs(selection: .importing, onSelect: { _ in })
        },
    ]
}
