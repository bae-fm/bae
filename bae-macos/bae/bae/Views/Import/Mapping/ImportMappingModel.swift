import BaeKit
import Foundation

/// One row of the slot table, as the pane renders it: the source's position,
/// the audio bound to it, the two lengths, and the editor row it edits.
///
/// `index` addresses both `BridgeSlotTable.rows` and the editor's
/// `BridgeRawReleaseEdit.tracks` — core builds them positionally aligned, so
/// row `i` edits edit-row `i`.
struct ImportSlotRow: Identifiable {
    let index: Int
    /// The source's own position string; `nil` for audio the source does not
    /// name, which the row leaves blank.
    let position: String?
    /// The audio this row will write, resolved from what the editor holds. A
    /// row the user re-pointed with "Choose file…" reads as paired here even
    /// though core built it as a track with nothing behind it.
    let file: BridgeSlotFile?
    let sourceDurationMs: UInt64?
    let title: String
    /// Whether committing writes a track for this row.
    let writesTrack: Bool

    var id: Int { index }

    /// A row that carries a file but no name yet — what the commit bar counts
    /// as unanswered.
    var isUnanswered: Bool {
        writesTrack
            && title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

/// The slot table and the editor after rows leave the import.
struct ImportRowRemoval {
    let slots: BridgeSlotTable?
    let edit: BridgeRawReleaseEdit
}

/// Everything the mapping pane's four zones render, projected once from the
/// candidate's files, the picked release's slot table, and the live editor.
///
/// The pane derives nothing about a file's job or a track's pairing: those
/// arrive as typed values. What it does here is place rows and count them.
struct ImportMappingModel {
    let roleRows: [ImportRoleRow]
    let slotRows: [ImportSlotRow]
    /// The tally above the slot table; `nil` when no release is picked, so
    /// there is nothing to reconcile the folder against.
    let reconciliation: BridgeSlotReconciliation?
    /// Every audio unit the folder offers — what "Choose file…" picks from.
    let audioChoices: [BridgeSlotFile]

    /// Rows that will become tracks: the ones carrying a file.
    var willWriteCount: Int {
        slotRows.filter(\.writesTrack).count
    }

    /// Rows that will become tracks whose title is still blank.
    var unansweredCount: Int {
        slotRows.filter(\.isUnanswered).count
    }

    /// Project the pane.
    ///
    /// `slots` is absent for an import with no picked release behind it — an
    /// "Add as Unknown" commit, whose tracklist comes from the files' own tags.
    /// There is no mapping to show then and no tally to state, and every editor
    /// row becomes a track, because the commit reads the folder rather than a
    /// binding the user left.
    init(
        files: BridgeCandidateFiles,
        slots: BridgeSlotTable?,
        edit: BridgeRawReleaseEdit?
    ) {
        roleRows = ImportRoleRow.rows(of: files)
        reconciliation = slots?.reconciliation
        audioChoices = slots?.audio ?? []

        guard let edit else {
            slotRows = []
            return
        }
        let rows = slots?.rows
        let audio = slots?.audio ?? []
        slotRows = edit.tracks.indices.map { index in
            let slot = rows.flatMap {
                $0.indices.contains(index) ? $0[index] : nil
            }
            let file = Self.file(
                bound: edit.tracks[index].file,
                slot: slot,
                audio: audio
            )
            return ImportSlotRow(
                index: index,
                position: slot?.position,
                file: file,
                sourceDurationMs: slot?.sourceDurationMs,
                title: edit.tracks[index].title,
                writesTrack: slots == nil ? true : file != nil
            )
        }
    }

    /// The audio a row displays: whatever the editor holds for it, described by
    /// the slot file core computed for that unit. The slot's own file answers
    /// first — it is the same unit for every row core paired — and the folder's
    /// offered audio answers for a row the user re-pointed.
    private static func file(
        bound: BridgeAudioFile?,
        slot: BridgeTrackSlot?,
        audio: [BridgeSlotFile]
    ) -> BridgeSlotFile? {
        guard let bound else { return nil }
        if let own = slot?.boundFile, own.audio == bound { return own }
        return audio.first { $0.audio == bound }
    }

    /// Take `fileId` out of the tracklist: drop every slot row it backs (one
    /// container backs several) and the editor rows at the same indices, and
    /// stop offering its audio to the rows that are left.
    ///
    /// The tally is recomputed from what is left rather than re-read from core,
    /// because the only way to get core's number back is another prefetch, and
    /// that would discard the user's edits. The recomputation is core's own
    /// rule — a row carrying a file is a file, a row carrying a position is a
    /// track — so an untouched table reproduces the number core sent.
    static func excluding(
        fileId: String,
        from slots: BridgeSlotTable,
        edit: BridgeRawReleaseEdit
    ) -> ImportRowRemoval {
        let dropped = Set(
            slots.rows.indices.filter {
                slots.rows[$0].boundFile?.audio.fileId == fileId
            }
        )
        let offered = slots.audio.filter { $0.audio.fileId != fileId }
        return removing(
            rowsAt: dropped,
            slots: BridgeSlotTable(
                rows: slots.rows,
                reconciliation: slots.reconciliation,
                audio: offered
            ),
            edit: edit
        )
    }

    /// Drop one row from the import — the "Drop" action on a track the source
    /// names that this folder has nothing for. The slot row and the editor row
    /// leave together: they are the same row.
    static func dropping(
        rowAt index: Int,
        slots: BridgeSlotTable?,
        edit: BridgeRawReleaseEdit
    ) -> ImportRowRemoval {
        removing(rowsAt: [index], slots: slots, edit: edit)
    }

    private static func removing(
        rowsAt dropped: Set<Int>,
        slots: BridgeSlotTable?,
        edit: BridgeRawReleaseEdit
    ) -> ImportRowRemoval {
        var nextEdit = edit
        nextEdit.tracks = edit.tracks.indices
            .filter { !dropped.contains($0) }
            .map { edit.tracks[$0] }
        guard let slots else {
            return ImportRowRemoval(slots: nil, edit: nextEdit)
        }
        let rows = slots.rows.indices
            .filter { !dropped.contains($0) }
            .map { slots.rows[$0] }
        return ImportRowRemoval(
            slots: BridgeSlotTable(
                rows: rows,
                reconciliation: reconciliation(of: rows),
                audio: slots.audio
            ),
            edit: nextEdit
        )
    }

    /// Core's tally rule over a set of rows: how many carry a file against how
    /// many the source names.
    private static func reconciliation(
        of rows: [BridgeTrackSlot]
    ) -> BridgeSlotReconciliation {
        let files = UInt32(rows.filter { $0.boundFile != nil }.count)
        let tracks = UInt32(rows.filter { $0.position != nil }.count)
        if files == tracks {
            return .agrees(count: files)
        }
        return files > tracks
            ? .moreFiles(files: files, tracks: tracks)
            : .moreTracks(files: files, tracks: tracks)
    }
}
