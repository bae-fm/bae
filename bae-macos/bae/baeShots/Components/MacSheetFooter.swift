import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSheetFooter`, a sheet's last row.
    static let macSheetFooterScenes: [ShotScene] = [
        component("mac.component.sheet-footer.plain", width: 842, height: 60) {
            MacSheetFooter {
                MacButton("Cancel") {}
                MacButton("Set up", kind: .primary) {}
            }
        },
        component("mac.component.sheet-footer.split", width: 842, height: 60) {
            MacSheetFooter {
                MacButton("New folder…") {}
            } trailing: {
                MacButton("Cancel") {}
                MacButton("Set up", kind: .primary) {}
            }
        },
        component(
            "mac.component.sheet-footer.two-on-the-left",
            width: 842,
            height: 60
        ) {
            MacSheetFooter {
                MacButton("New folder…") {}
                MacButton("Look again") {}
            } trailing: {
                MacButton("Cancel") {}
                MacButton("Set up", kind: .primary) {}
            }
        },
    ]
}
