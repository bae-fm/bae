import BaeKit
import Foundation

/// What the mapping table's rows call back into. One table, one set of actions:
/// the left half's decisions about the folder, and the right half's about the
/// tracklist being committed.
struct ImportMappingActions {
    /// Put a file in a role, or put it back: the file's id, then the choice.
    /// Core persists it, and the table is re-read because a role change is a
    /// different set of rows.
    let setRole: (String, BridgeFileRoleChoice) -> Void
    /// Associate a sheet's FILE reference with audio: sheet id, reference,
    /// and audio id, or `nil` to clear that reference.
    let bindSheet: (String, String, String?) -> Void
    /// Say which disc of the release a track sheet's entries are, or take them
    /// out of the tracklist: the sheet's file id, then the assignment.
    let setSheetDisc: (String, BridgeSheetDisc) -> Void
    /// Open a document (a log, a text file, a track sheet) in the viewer: the
    /// file's name, then its path on disk.
    let openDocument: (String, String) -> Void
    /// Open the folder's images in the lightbox: the gallery's images, then
    /// the path of the one that was clicked.
    let openImages: ([BridgeMappingImage], String) -> Void
    /// Audition the exact source window carried by a mapping row.
    let preview: (BridgePreviewTarget) -> Void
    /// Stop whatever is auditioning.
    let stopPreview: () -> Void
    /// Write a row's edited track back onto the row that commits it.
    let editTrack: (BridgeRawTrackEdit) -> Void
    /// Point a row at one of the folder's audio units: the row's track id,
    /// then the unit.
    let chooseFile: (String, BridgeAudioFile) -> Void
    /// Delete a draft track without changing the file on disk.
    let drop: (String) -> Void
}

/// What the commit bar calls back into.
struct ImportCommitActions {
    let confirmImport: () -> Void
    let mergeArtists: (String) -> Void
    let viewInLibrary: (String) -> Void
}
