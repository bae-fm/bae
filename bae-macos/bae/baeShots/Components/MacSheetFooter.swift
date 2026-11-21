import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSheetFooter`, a sheet's last row.
    static let macSheetFooterScenes: [ShotScene] = [
        component("mac.component.sheet-footer.plain", height: 60) {
            MacSheetFooter {
                Button("Cancel") {}
                Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.sheet-footer.split", height: 60) {
            MacSheetFooter {
                Button("New folder…") {}
            } trailing: {
                Button("Cancel") {}
                Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.sheet-footer.two-on-the-left", height: 60) {
            MacSheetFooter {
                Button("New folder…") {}
                Button("Look again") {}
            } trailing: {
                Button("Cancel") {}
                Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380, alignment: .leading)
        },
    ]
}
