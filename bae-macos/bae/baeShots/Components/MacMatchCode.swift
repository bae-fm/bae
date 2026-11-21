import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMatchCode`, one shot for each row of its preview.
    static let macMatchCodeScenes: [ShotScene] = [
        // `MacMatchCode` with the line that says what this device waits for.
        component("mac.component.match-code.waiting", height: 110) {
            MacMatchCode(
                code: "412 778",
                waiting: "Waiting for your other device…"
            )
            .frame(width: 380)
        },
        // `MacMatchCode`, the digits.
        component("mac.component.match-code.alone", height: 70) {
            MacMatchCode(code: "412 778")
        },
        component("mac.component.match-code.dialog", height: 110) {
            MacMatchCode(
                code: "412 778",
                waiting: "Waiting for your other device…"
            )
            .frame(width: 380)
        },
    ]
}
