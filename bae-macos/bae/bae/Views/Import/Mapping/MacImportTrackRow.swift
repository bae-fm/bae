import BaeKit
import SwiftUI

/// One row of the release editor's track table: what plays the track, its
/// place, and its title and artists, which edit in place.
struct MacImportTrackRow: View {
    let row: BridgeTrackTableRow
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    let actions: ReleaseTrackTableActions
    /// Opens the live artist search the track's artist field reads while
    /// open.
    let artistSearch: ArtistSearchOpener

    private var isPreviewing: Bool {
        guard let target = row.audition.previewTarget else { return false }
        return target == previewingTarget
    }

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            ReleaseMetadataTrackRow(
                track: row.track,
                artist: row.artist,
                length: row.length,
                listedLength: row.listedLength,
                columns: columns,
                editingCommands: editingCommands,
                position: row.position,
                artistSearch: artistSearch,
                onChange: { await actions.editTrack($0) }
            )
        }
        .padding(.vertical, ThemeSpace.compact)
        .frame(minHeight: 40)
        .padding(.top, ThemeSpace.hairline)
        .background(isPreviewing ? Theme.accentSoft : .clear)
        .overlay(alignment: .top) { MacHairline() }
    }

    /// The file or CUE slice that supplies this track's audio.
    private var sourceCell: some View {
        TrackSourceCell(
            source: row.source,
            audition: row.audition,
            trackId: row.track.id,
            isPreviewing: isPreviewing,
            actions: actions.audition,
        )
        .frame(width: columns.source, alignment: .leading)
        // Double-clicking anywhere in the cell auditions a folder's row;
        // simultaneous so the play glyph's single click is not delayed.
        .contentShape(Rectangle())
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    if let target = row.audition.previewTarget {
                        actions.audition.preview(target)
                    }
                }
        )
    }
}
