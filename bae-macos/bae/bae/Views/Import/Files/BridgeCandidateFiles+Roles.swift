import BaeKit
import Foundation

/// Reading a candidate's files by the role core proposed for each of them. The
/// role and the sheet↔audio binding cross the bridge as typed values: nothing
/// here decides a file's job, and nothing infers a pairing from a filename.

extension BridgeSheetBindingOffer {
    /// Why a file can't back the sheet, in the user's language; nil when it
    /// can. Core decides both the refusal and its wording — the picker only
    /// places the line and dims the row.
    var refusalLine: String? {
        guard let key = bridgeSheetBindingOfferKey(offer: self) else {
            return nil
        }
        guard case .refusedCodec(let codec) = self else {
            return localizedCoreString(key)
        }
        return localizedCoreString(key, codec)
    }
}
