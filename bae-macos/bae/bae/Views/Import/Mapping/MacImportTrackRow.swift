import BaeKit
import SwiftUI

/// One of the folder's audio units alongside the track it becomes. The track's
/// title and artist edit in place; its audio is the folder's.
struct MacImportTrackRow: View {
    let row: BridgeTrackTableRow
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    let actions: ImportMappingActions
    /// Opens the live artist search the track's artist field reads while
    /// open.
    let artistSearch: ArtistSearchOpener

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            ReleaseMetadataTrackRow(
                track: row.track,
                artists: row.artists,
                length: row.length,
                listedLength: row.listedLength,
                columns: columns,
                editingCommands: editingCommands,
                position: row.position,
                artistSearch: artistSearch,
                onChange: { actions.editTrack($0) }
            )
        }
    }

    /// The file or CUE slice that supplies this track's audio.
    private var sourceCell: some View {
        ImportMappingSourceCell(
            source: row.source,
            previewTarget: row.audition.previewTarget,
            previewingTarget: previewingTarget,
            actions: actions,
        )
        .frame(width: columns.source, alignment: .leading)
        // Double-clicking anywhere in the cell auditions; simultaneous so the
        // play glyph's single click is not delayed.
        .contentShape(Rectangle())
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    if let target = row.audition.previewTarget {
                        actions.preview(target)
                    }
                }
        )
    }
}
