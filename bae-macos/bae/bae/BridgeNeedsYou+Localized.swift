import BaeKit
import Foundation

extension BridgeNeedsYou {
    /// The row's disagreement sentence, resolved from the generated `Core`
    /// string table via the key bae-core owns for this variant
    /// (`bridgeNeedsYouKey`). Every number crosses raw — the enum's own
    /// operands — so this is the one place they're interpolated for the
    /// current locale.
    var localizedText: String {
        let template = NSLocalizedString(
            bridgeNeedsYouKey(needsYou: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
        switch self {
        case .severalMatches(let count):
            return String.localizedStringWithFormat(template, Int(count))
        case .trackCountDisagrees(let local, let source):
            return String.localizedStringWithFormat(
                template,
                Int(local),
                Int(source)
            )
        case .mediumDisagrees(.notCdAudio(let sampleRateHz)):
            let kilohertz = (Double(sampleRateHz) / 1000)
                .formatted(.number.precision(.fractionLength(0...1)))
            return String.localizedStringWithFormat(template, kilohertz)
        case .noMatch, .nothingToLookUp,
            .lookupFailed, .sourceTracksUnknown, .mediumDisagrees(.cdRip),
            .mediumDisagrees(.monoAudio):
            return template
        }
    }
}
