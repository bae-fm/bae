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

    var isTrackSheet: Bool {
        if case .trackSheet = self { return true }
        return false
    }

    /// Cover or artwork — everything the gallery and the cover picker show.
    var isImage: Bool {
        switch self {
        case .cover, .artwork: return true
        default: return false
        }
    }

    var isDocument: Bool {
        if case .document = self { return true }
        return false
    }
}

extension BridgeCandidateFile {
    /// The cover choice this file offers the picker; nil when it isn't an image.
    var coverChoice: BridgeCoverChoice? {
        switch role {
        case .cover(let choice): return choice
        case .artwork(let choice): return choice
        default: return nil
        }
    }

}

extension BridgeCandidateFiles {
    /// The sheets whose bindings the mapping table offers a control for — one
    /// read of core's offers per sheet.
    var trackSheets: [BridgeCandidateFile] {
        files.filter { $0.role.isTrackSheet }
    }

    /// Cover and artwork — what the cover picker and the lightbox show.
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
