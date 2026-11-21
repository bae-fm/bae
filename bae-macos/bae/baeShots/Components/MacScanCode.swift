import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacScanCode`, the QR code of a short code with the buttons that copy
    /// and share it.
    static let macScanCodeScenes: [ShotScene] = [
        component("mac.component.scan-code.default", page: true, height: 330) {
            MacScanCode(
                instruction:
                    "Scan the QR code on their device, or share the code.",
                qr: "bae-pairing-4F7K-2M9Q",
                code: "4F7K-2M9Q"
            )
            .frame(width: 380, alignment: .leading)
        }
    ]
}
