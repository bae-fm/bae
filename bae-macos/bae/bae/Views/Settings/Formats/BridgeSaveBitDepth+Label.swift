import BaeKit

extension BridgeSaveBitDepth {
    var label: String {
        switch self {
        case .sameAsSource: String(localized: "Same as source")
        case .bits16: String(localized: "16-bit")
        }
    }
}
