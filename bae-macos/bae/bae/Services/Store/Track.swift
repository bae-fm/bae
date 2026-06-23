import Foundation

struct Track: Identifiable {
    let id: String
    var title: String
    var durationMs: Int64?
    var artistNames: String
    var position: BridgeTrackPosition

    var durationLabel: String { DurationClock.text(durationMs) }

    /// The position string composed mechanically from the structured case:
    /// "A1" (sided), "2-3" (multi-disc), "5" (flat). No translatable word,
    /// so the UI builds it directly; numbers format per locale.
    var positionText: String { position.positionText }

    init(from bridge: BridgeTrack) {
        id = bridge.id
        title = bridge.title
        durationMs = bridge.durationMs
        artistNames = bridge.artistNames
        position = bridge.position
    }
}

extension BridgeTrackPosition {
    /// Mechanical compose of the position string from the case. Numbers format
    /// for the current locale; the side letter is a bare proper-noun letter.
    var positionText: String {
        switch self {
        case .sided(let sideLetter, let number):
            guard let number else {
                preconditionFailure("track position has no track number")
            }
            return "\(sideLetter)\(number.formatted())"
        case .disc(let disc, let number):
            guard let number else {
                preconditionFailure("track position has no track number")
            }
            return "\(disc.formatted())-\(number.formatted())"
        case .flat(let number):
            guard let number else {
                preconditionFailure("track position has no track number")
            }
            return number.formatted()
        @unknown default:
            preconditionFailure(
                "unhandled BridgeTrackPosition case in positionText"
            )
        }
    }
}
