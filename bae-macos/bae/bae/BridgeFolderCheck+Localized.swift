import BaeKit
import Foundation

extension BridgeFolderCheck {
    /// The failed check's sentence, resolved from the generated `Core` string
    /// table via the key bae-core owns for this variant
    /// (`bridgeFolderCheckKey`). Every number crosses raw — the enum's own
    /// operands — so this is the one place they're interpolated for the
    /// current locale. `nil` for audio no CD holds whose rate is not at hand,
    /// which a sentence stating the rate cannot be written without.
    var localizedText: String? {
        let template = NSLocalizedString(
            bridgeFolderCheckKey(folderCheck: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
        switch self {
        case .trackCountDisagrees(let local, let source):
            return String.localizedStringWithFormat(
                template,
                Int(local),
                Int(source)
            )
        case .mediumDisagrees(.notCdAudio(let sampleRateHz)):
            guard let sampleRateHz else { return nil }
            let kilohertz = (Double(sampleRateHz) / 1000)
                .formatted(.number.precision(.fractionLength(0...1)))
            return String.localizedStringWithFormat(template, kilohertz)
        case .sourceTracksUnknown, .mediumDisagrees(.cdRip):
            return template
        }
    }
}
