import BaeKit
import Foundation

/// Reading a candidate's files by the role core proposed for each of them. The
/// role and the sheet↔audio binding cross the bridge as typed values: nothing
/// here decides a file's job, and nothing infers a pairing from a filename.

extension BridgeFileRole {
    var isAudio: Bool {
        if case .audio = self { return true }
        return false
    }

    /// Artwork shown in the gallery, including images unavailable as covers.
    var isImage: Bool {
        if case .artwork = self { return true }
        return false
    }

    var isDocument: Bool {
        if case .document = self { return true }
        return false
    }
}

extension BridgeCandidateFile {
    /// Core's cover action; nil for files the cover decoder does not support.
    var coverChoice: BridgeCoverChoice? {
        switch role {
        case .artwork(let choice): return choice
        default: return nil
        }
    }

}

extension BridgeCandidateFiles {
    /// Every artwork attachment shown by the lightbox.
    var images: [BridgeCandidateFile] { files.filter { $0.role.isImage } }
}

extension BridgeSheetBindingOption {
    /// Why this file can't back the sheet, in the user's language; nil when it
    /// can. Core decides both the refusal and its wording — the picker only
    /// places the line and dims the row.
    var refusalLine: String? {
        guard let key = bridgeSheetBindingOfferKey(offer: offer) else {
            return nil
        }
        guard case .refusedCodec(let codec) = offer else {
            return coreString(key)
        }
        return coreString(key, codec)
    }
}
