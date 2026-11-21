import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacRecordLinks`, the catalogs a release is described by.
    static let macRecordLinksScenes: [ShotScene] = [
        component("mac.component.record-links.two-catalogs", height: 50) {
            MacRecordLinks(records: PreviewData.releaseRecordsPair)
                .frame(width: 380, alignment: .leading)
        },
        component("mac.component.record-links.wrapped", height: 90) {
            MacRecordLinks(records: PreviewData.releaseRecordsEveryCatalog)
                .frame(width: 300, alignment: .leading)
        },
    ]
}
