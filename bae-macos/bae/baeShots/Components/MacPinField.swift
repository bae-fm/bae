import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPinField`, one box per digit of a receiver's PIN.
    static let macPinFieldScenes: [ShotScene] = [
        pinField("mac.component.pin-field.empty", value: ""),
        pinField("mac.component.pin-field.focused", value: "48", focused: true),
        pinField("mac.component.pin-field.filled", value: "4821"),
        pinField(
            "mac.component.pin-field.error",
            value: "4821",
            error: "That PIN did not match."
        ),
        pinField("mac.component.pin-field.off", value: "4821", off: true),
        pinField("mac.component.pin-field.six-digits", digits: 6, value: "482"),
    ]

    private static func pinField(
        _ id: String,
        digits: Int = 4,
        value: String,
        error: String? = nil,
        focused: Bool = false,
        off: Bool = false
    ) -> ShotScene {
        component(id, height: error == nil ? 80 : 100) {
            MacPinField(
                digits: digits,
                value: .constant(value),
                error: error,
                focusesOnAppear: focused
            )
            .disabled(off)
        }
    }
}
