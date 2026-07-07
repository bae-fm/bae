import Foundation
import os.log

private let logger = Logger.bae("TrackGroup")

public struct TrackGroup {
    public var side: BridgeTrackSide
    public var tracks: [Track]

    /// The group header for the current locale: "Side A" / "Disc 2", or empty
    /// for a flat single-disc group (no header). bae-core decides the case and
    /// the side letter / disc number; the UI resolves the "Side"/"Disc" word
    /// from the catalog key and substitutes the number.
    public var sideHeaderText: String { side.sideHeaderText }

    public init(from bridge: BridgeTrackGroup) {
        side = bridge.side
        tracks = bridge.tracks.map(Track.init(from:))
    }
}

extension BridgeTrackSide {
    /// The localized group header ("Side A" / "Disc 2"), or empty for the flat
    /// case. The word comes from the catalog key bae-core owns; the side letter
    /// / disc number is substituted in.
    public var sideHeaderText: String {
        guard let key = bridgeTrackHeaderKey(side: self) else { return "" }
        let format = NSLocalizedString(
            key,
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
        switch self {
        case .sided(let sideLetter):
            return String(format: format, sideLetter)
        case .disc(let disc):
            // The "Disc {disc}" catalog value compiles to "Disc %lld"; pass a
            // 64-bit Int so the C-variadic width matches (Int32 would misread).
            return String(format: format, Int(disc))
        case .flat:
            // Unreachable: the guard already returned for the keyless flat
            // side. Swift still requires the case for exhaustiveness.
            return ""
        @unknown default:
            logger.warning("unhandled BridgeTrackSide case in sideHeaderText")
            return ""
        }
    }
}
