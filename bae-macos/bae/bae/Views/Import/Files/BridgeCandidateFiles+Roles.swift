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

    /// Unrecognized, and carried with the release anyway.
    var isOther: Bool {
        if case .other = self { return true }
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

    /// What this track sheet's `FILE` directive resolved to; nil when the file
    /// isn't a sheet.
    var sheetBinding: BridgeSheetBinding? {
        if case .trackSheet(let binding, _) = role { return binding }
        return nil
    }

    /// The audio this track sheet describes, named by that file's
    /// release-relative path. Nil unless the sheet is bound to playable audio.
    var describes: String? {
        guard case .describes(let fileId)? = sheetBinding else { return nil }
        return fileId
    }

    /// Why this sheet describes nothing, in the user's language; nil when it is
    /// bound, or when the file isn't a sheet. Core owns both the reason and its
    /// wording — the pane only places the line.
    var unboundSheetLine: String? {
        switch sheetBinding {
        case .none, .describes:
            return nil
        case .unresolved:
            return String(localized: "Describes nothing yet")
        case .refusedCodec(_, let codec):
            let format = NSLocalizedString(
                bridgeSheetRefusedCodecKey(),
                tableName: "Core",
                bundle: .main,
                comment: ""
            )
            return String(format: format, codec)
        }
    }

    /// Playable tracks the sheet carves; 0 when the file isn't a sheet.
    var sheetTrackCount: UInt32 {
        if case .trackSheet(_, let trackCount) = role { return trackCount }
        return 0
    }
}

extension BridgeCandidateFiles {
    var audioFiles: [BridgeCandidateFile] { files.filter { $0.role.isAudio } }
    var trackSheets: [BridgeCandidateFile] {
        files.filter { $0.role.isTrackSheet }
    }
    var images: [BridgeCandidateFile] { files.filter { $0.role.isImage } }
    var documents: [BridgeCandidateFile] { files.filter { $0.role.isDocument } }
    var other: [BridgeCandidateFile] { files.filter { $0.role.isOther } }

    /// The track sheets bound to `fileId`, in the folder's own file order.
    func sheets(describing fileId: String) -> [BridgeCandidateFile] {
        trackSheets.filter { $0.describes == fileId }
    }

    /// Sheets whose `FILE` directive named audio that isn't in the folder.
    var unboundTrackSheets: [BridgeCandidateFile] {
        trackSheets.filter { $0.describes == nil }
    }
}
