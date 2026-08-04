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
    /// Name the audio a track sheet describes: the sheet's file id, then the
    /// audio's, or `nil` to leave the sheet describing nothing.
    let bindSheet: (String, String?) -> Void
    /// Say which disc of the release a track sheet's entries are, or take them
    /// out of the tracklist: the sheet's file id, then the assignment.
    let setSheetDisc: (String, BridgeSheetDisc) -> Void
    /// Open a document (a log, a text file, a track sheet) in the viewer: the
    /// file's name, then its path on disk.
    let openDocument: (String, String) -> Void
    /// Open the folder's images in the lightbox: the gallery's images, then
    /// the path of the one that was clicked.
    let openImages: ([BridgeMappingImage], String) -> Void
    /// Audition a row's audio from its own path.
    let preview: (String) -> Void
    /// Stop whatever is auditioning.
    let stopPreview: () -> Void
    /// Write a row's edited track back onto the row that commits it.
    let editTrack: (BridgeRawTrackEdit) -> Void
    /// Point a row at one of the folder's audio units: the row's track id,
    /// then the unit.
    let chooseFile: (String, BridgeAudioFile) -> Void
    /// Remove a row from the import entirely — a track the release names that
    /// this folder has nothing for.
    let drop: (String) -> Void
    /// Take a file out of the tracklist by id. Persisted: it is a fact about
    /// the folder, so it survives re-picking a release.
    let exclude: (String) -> Void
}

/// What the commit bar calls back into.
struct ImportCommitActions {
    let confirmImport: () -> Void
    let viewInLibrary: (String) -> Void
}
