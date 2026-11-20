import Foundation

extension BridgeSideHeading {
    /// The localized heading of a side or disc ("Side A", "Disc 2"). Every
    /// screen, prompt and error that names a side or disc words it here.
    public var text: String {
        switch self {
        case .sided(let sideLetter):
            return String(
                format: localizedCoreString("core.track.side"),
                sideLetter
            )
        case .disc(let disc):
            return String(
                format: localizedCoreString("core.track.disc"),
                Int(disc)
            )
        @unknown default:
            fatalError("Unhandled BridgeSideHeading case")
        }
    }
}

extension BridgeTrackSide {
    /// The localized heading of a side or disc, or an empty string for a flat
    /// side, which has no heading.
    public var headingText: String {
        switch self {
        case .sided(let sideLetter):
            return BridgeSideHeading.sided(sideLetter: sideLetter).text
        case .disc(let disc):
            return BridgeSideHeading.disc(disc: disc).text
        case .flat:
            return ""
        @unknown default:
            fatalError("Unhandled BridgeTrackSide case")
        }
    }
}
