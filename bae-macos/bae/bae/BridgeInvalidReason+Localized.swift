import BaeKit
import Foundation

extension BridgeInvalidReason {
    /// The localized failure reason, resolved from the generated `Core` string
    /// table via the key bae-core owns, with the name or the count of damaged
    /// files where the reason carries one.
    var localizedText: String {
        let key = bridgeInvalidReasonKey(reason: self)
        switch self {
        case .sameName(let name):
            return coreString(key, name)
        case .damagedAudio(let count):
            return coreString(key, Int(count))
        case .noValidAudio:
            return coreString(key)
        }
    }
}

extension BridgeAudioDamage {
    /// Why the audio file can't be played, in the user's language.
    var localizedText: String {
        coreString(bridgeAudioDamageKey(damage: self))
    }
}

extension BridgeSingleSelection {
    /// The selected candidate's key, when the one selected row is one.
    var candidateKey: String? {
        guard case .candidate(let key) = self else { return nil }
        return key
    }

    /// The selected folder's key, when the one selected row failed
    /// validation.
    var invalidCandidateKey: String? {
        guard case .invalidCandidate(let key) = self else { return nil }
        return key
    }
}
