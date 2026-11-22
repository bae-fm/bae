import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportGroupHeader`, one shot for each row of its preview.
    static let macImportGroupHeaderScenes: [ShotScene] = [
        component(
            "mac.component.import-group-header.default",
            page: true,
            height: 60
        ) {
            MacImportGroupHeader(
                group: BridgeTriageGroup(
                    key: PreviewData.releaseQueueGroupKey,
                    name: "Mara Quill — Field Recordings (1979)",
                    combinable: false
                ),
                expanded: true,
                onSetExpanded: { _ in },
                onCombine: {}
            )
            .frame(width: 380, height: 40)
        },
        component(
            "mac.component.import-group-header.folded",
            page: true,
            height: 60
        ) {
            MacImportGroupHeader(
                group: BridgeTriageGroup(
                    key: PreviewData.releaseQueueGroupKey,
                    name: "Mara Quill — Field Recordings (1979)",
                    combinable: false
                ),
                expanded: false,
                onSetExpanded: { _ in },
                onCombine: {}
            )
            .frame(width: 380, height: 40)
        },
    ]
}
