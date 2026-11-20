import Foundation

extension BridgeTrackSide {
    /// The localized heading of a side or disc ("Side A", "Disc 2"), or an
    /// empty string for a flat side, which has no heading. Core names the
    /// catalog key; every screen, prompt and error that names a side or disc
    /// words it here.
    public var headingText: String {
        guard let key = bridgeTrackSideHeadingKey(side: self) else { return "" }
        let format = localizedCoreString(key)
        switch self {
        case .sided(let sideLetter):
            return String(format: format, sideLetter)
        case .disc(let disc):
            return String(format: format, Int(disc))
        case .flat:
            return ""
        @unknown default:
            fatalError("Unhandled BridgeTrackSide case")
        }
    }
}
