import BaeKit
import SwiftUI

/// One of the folder's audio units alongside the track it becomes. The track's
/// title and artist edit in place; its audio is the folder's.
struct ImportMappingTrackRow: View {
    let mapping: BridgeTrackMapping
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    /// Identifying signals extracted from this row's file. Empty for every
    /// other row.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            ReleaseMetadataTrackRow(
                track: mapping.track,
                duration: mapping.displayedDuration,
                durationDiverges: mapping.lengthsDisagree,
                columns: columns,
                editingCommands: editingCommands,
                displayedPosition: mapping.position,
                onChange: { actions.editTrack($0) }
            )
        }
    }

    /// The file or CUE slice that supplies this track's audio.
    private var sourceCell: some View {
        ImportMappingSourceCell(
            source: mapping.source,
            previewingTarget: previewingTarget,
            evidence: evidence,
            showsFileSize: false,
            actions: actions,
        )
        .frame(width: columns.source, alignment: .leading)
        // Double-clicking anywhere in the cell auditions; simultaneous so the
        // play glyph's single click is not delayed.
        .contentShape(Rectangle())
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    if let target = mapping.source.previewTarget {
                        actions.preview(target)
                    }
                }
        )
    }
}
