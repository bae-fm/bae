import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportDestination`, the pop-up of where an import goes.
    static let macImportDestinationScenes: [ShotScene] = [
        component("mac.component.import-destination.this-mac", height: 60) {
            MacImportDestination(destination: .constant(.thisDevice))
        },
        component("mac.component.import-destination.cloud", height: 60) {
            MacImportDestination(destination: .constant(.cloud))
        },
        component("mac.component.import-destination.off", height: 60) {
            MacImportDestination(destination: .constant(.cloud))
                .disabled(true)
        },
    ]
}
