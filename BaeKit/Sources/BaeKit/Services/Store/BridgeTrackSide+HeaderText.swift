import Foundation

extension BridgeTrackSide {
    /// The localized heading for a side or disc, or an empty string when the
    /// section is flat.
    public func headerText(key: String?) -> String {
        guard let key else { return "" }
        let format = NSLocalizedString(
            key,
            tableName: "Core",
            bundle: .module,
            comment: ""
        )
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
