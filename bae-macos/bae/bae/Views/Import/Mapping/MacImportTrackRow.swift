import BaeKit
import SwiftUI

/// One row of the release editor's track table: what plays the track, its
/// place, and its title and artists, which edit in place.
struct MacImportTrackRow: View {
    let row: BridgeTrackTableRow
    let hasAlbumArtists: Bool
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    let actions: ReleaseTrackTableActions
    /// Opens the live artist search the track's artist field reads while
    /// open.
    let artistSearch: ArtistSearchOpener

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            ReleaseMetadataTrackRow(
                track: row.track,
                artists: row.artists,
                hasAlbumArtists: hasAlbumArtists,
                length: row.length,
                listedLength: row.listedLength,
                columns: columns,
                editingCommands: editingCommands,
                position: row.position,
                artistSearch: artistSearch,
                onChange: { await actions.editTrack($0) }
            )
        }
    }

    /// The file or CUE slice that supplies this track's audio.
    private var sourceCell: some View {
        TrackSourceCell(
            source: row.source,
            audition: row.audition,
            trackId: row.track.id,
            previewingTarget: previewingTarget,
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
