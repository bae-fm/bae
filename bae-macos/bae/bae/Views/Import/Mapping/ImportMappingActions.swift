import BaeKit
import Foundation

/// What the roles table calls back into.
struct ImportRoleActions {
    /// Put a file in a role, or put it back: the file's id, then the choice.
    /// Core persists it and re-scans; nothing is updated here.
    let setRole: (String, BridgeFileRoleChoice) -> Void
    /// Name the audio a track sheet describes: the sheet's file id, then the
    /// audio's, or `nil` to leave the sheet describing nothing.
    let bindSheet: (String, String?) -> Void
    /// Open a document (a log, a text file, a track sheet) in the viewer.
    let openDocument: (BridgeFileInfo) -> Void
    /// Open the folder's images in the lightbox, at this file's path.
    let openImage: (String) -> Void
}

/// What the slot table calls back into.
struct ImportSlotActions {
    /// Audition the row's audio from its own path.
    let preview: (String) -> Void
    /// Stop whatever is auditioning.
    let stopPreview: () -> Void
    /// Bind a row to one of the folder's audio units: the row's index, then the
    /// unit.
    let chooseFile: (Int, BridgeAudioFile) -> Void
    /// Remove a row from the edit entirely — a track the source names that this
    /// folder has nothing for.
    let drop: (Int) -> Void
    /// Take a file out of the tracklist by id. Persisted: it is a fact about
    /// the folder, so it survives re-picking a release.
    let exclude: (String) -> Void
}

/// What the commit bar calls back into.
struct ImportCommitActions {
    let confirmImport: () -> Void
    let viewInLibrary: (String) -> Void
}
