import BaeKit
import Foundation

extension BridgeInvalidReason {
    /// The localized failure reason, resolved from the generated `Core` string
    /// table via the key bae-core owns, with the file path interpolated where
    /// the reason carries one.
    var localizedText: String {
        let format = NSLocalizedString(
            bridgeInvalidReasonKey(reason: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
        switch self {
        case .corruptAudioFile(let path), .corruptImage(let path),
            .cueParseFailed(let path):
            return String(format: format, path)
        case .cueUnsupportedCodec(let codec):
            return String(format: format, codec)
        case .cueMissingAudio, .cueUnsupportedLayout, .noValidAudio:
            return format
        }
    }
}
