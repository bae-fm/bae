import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the import that draw from values: the folder
    /// line, a group's header and the band of identifiers.
    static let importComponentScenes: [ShotScene] = [
        component("mac.component.import-folder-line.default", height: 50) {
            CandidateFolderLine(
                tab: nil,
                folderName: "Mara Quill - Low Tide Hours (1992) [FLAC]",
                folderPaths: [],
                onNavigateToPlacement: {}
            )
            .frame(width: 380)
        },
        component("mac.component.import-group-header.default", height: 60) {
            ImportListGroupHeader(
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
            "mac.component.identifier-band.default",
            width: 596,
            height: 100
        ) {
            MacIdentifierBand(
                run: PreviewData.identifyRunFound,
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in }
            )
        },
    ]
}
