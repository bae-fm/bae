import BaeKit
import Foundation

extension BridgeNeedsYou {
    /// The row's disagreement sentence, resolved from the generated `Core`
    /// string table via the key bae-core owns for this variant
    /// (`bridgeNeedsYouKey`). Every number crosses raw — the enum's own
    /// operands — so this is the one place they're interpolated for the
    /// current locale. Durations go through `DurationClock` first, matching
    /// how every other duration in the app renders; `tolerance_ms` never
    /// appears in the sentence even though `durationsDisagree` carries it —
    /// bae-core's own doc on the variant explains why.
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
        case .durationsDisagree(let probedMs, let sourceMs, _):
            return String.localizedStringWithFormat(
                template,
                DurationClock.text(Int64(probedMs)),
                DurationClock.text(Int64(sourceMs))
            )
        case .alreadyInLibrary, .noMatch, .nothingToLookUp,
            .lookupFailed, .sourceLengthsUnknown, .localDurationUnknown:
            return template
        }
    }
}
