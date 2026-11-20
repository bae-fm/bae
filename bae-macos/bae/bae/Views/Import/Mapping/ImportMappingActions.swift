import BaeKit
import Foundation

/// What the mapping table's rows call back into. One table, one set of actions:
/// the left half's decisions about the folder, and the right half's about the
/// tracklist being committed.
struct ImportMappingActions {
    /// Associate a sheet's FILE reference with audio: sheet id, reference,
    /// and audio id, or `nil` to clear that reference.
    let bindSheet: (String, String, String?) -> Void
    /// Say which disc of the release a track sheet's entries are, or take them
    /// out of the tracklist: the sheet's file id, then the assignment.
    let setSheetPlace: (String, BridgeSheetPlace) -> Void
    /// Open a document (a log, a text file, a track sheet) in the viewer: the
    /// file's name, then its file id.
    let openDocument: (String, String) -> Void
    /// Open the folder's images in the lightbox at the image whose file id
    /// it gets, the one the person clicked.
    let openImages: (String) -> Void
    /// Audition the exact source window carried by a mapping row.
    let preview: (BridgePreviewTarget) -> Void
    /// Stop whatever is auditioning.
    let stopPreview: () -> Void
    /// Write a row's edited title and artists back onto the row that commits
    /// it.
    let editTrack: @MainActor (BridgeRawTrackEdit) -> Void
}

extension ImportMappingActions {
    /// The track table's actions on the candidate: its sheets' places and
    /// bindings, the preview player, and its rows' titles and artists.
    var trackTable: ReleaseTrackTableActions {
        let editTrack = editTrack
        return ReleaseTrackTableActions(
            sheetCaption: sheetCaption,
            audition: audition,
            editTrack: { editTrack($0) }
        )
    }

    /// The preview player's actions; a folder's rows play no library track.
    var audition: TrackAuditionActions {
        TrackAuditionActions(
            preview: preview,
            stopPreview: stopPreview,
            playTrack: nil
        )
    }
}

/// What the commit bar calls back into.
struct ImportCommitActions {
    let confirmImport: () -> Void
    let viewInLibrary: (String) -> Void
}
