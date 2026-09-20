import BaeKit
import SwiftUI

/// One of the release's tracks: the file behind it, its number, what it will be
/// called, who it is by, and how long it runs.
///
/// The title and artist are edited in place — this is the release being
/// written. Every draft row has audio; its context menu can change that audio.
struct ImportMappingTrackRow: View {
    @Environment(\.sourceFileEditsAllowed)
    private var sourceFileEditsAllowed
    let mapping: BridgeTrackMapping
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    /// Included audio units available for swapping between tracks.
    let audioChoices: [ImportAudioChoice]
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    /// Identifying signals extracted from this row's file. Empty for every
    /// other row.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions

    @State
    private var hovering = false

    /// Whether the folder and the release disagree about how long this row
    /// runs. Core decides how far apart is far enough — it is a judgement about
    /// how much two rips of one track may legitimately differ, and the other
    /// desktop surface has to reach the same answer.
    private var lengthsDiverge: Bool {
        mapping.durationsDiverge
    }

    /// The track this row writes, where a release has named one.
    private var track: BridgeRawTrackEdit? {
        if case .track(let track, _, _) = mapping.becomes { return track }
        return nil
    }

    private var displayedPosition: String {
        if case .track(_, let position, _) = mapping.becomes { return position }
        return ""
    }

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            if let track {
                ReleaseMetadataTrackRow(
                    track: track,
                    duration: mapping.displayedDuration,
                    durationDiverges: lengthsDiverge,
                    columns: columns,
                    editingCommands: editingCommands,
                    displayedPosition: displayedPosition,
                    onChange: { actions.editTrack($0) }
                )
            }
            else {
                awaitingTrackCells
            }
            removalCell
        }
        // The whole row is the hover shape, gaps included. Hover follows
        // hit-testing, and a stack's empty space is not hit-testable on its
        // own — without this, the pointer crossing a gap on its way to the
        // removal X ends the hover that shows the X.
        .contentShape(Rectangle())
        .onHover {
            hovering = $0
        }
        .contextMenu {
            if sourceFileEditsAllowed, let track, !audioChoices.isEmpty {
                chooseFileButtons(track)
            }
        }
    }

    @ViewBuilder
    private var awaitingTrackCells: some View {
        Color.clear.frame(width: ReleaseMetadataTrackColumns.track)
        Text(coreString("ui.import.becomes.awaiting_pick"))
            .font(.system(size: 12))
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .frame(width: columns.title, alignment: .leading)
        Color.clear.frame(width: columns.artist)
        Text(mapping.displayedDuration)
            .font(.system(size: 12))
            .monospacedDigit()
            .frame(
                width: ReleaseMetadataTrackColumns.length,
                alignment: .trailing
            )
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
        // The whole cell auditions on double-click, not only the play glyph
        // — the filename is the biggest target the row has. Simultaneous, so
        // the glyph's own single click is not held back for a second one.
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

    /// Deleting a draft track leaves its physical file available.
    private var removalCell: some View {
        ZStack {
            if let track, let removal = removal(track) {
                ImportMappingRowRemovalButton(
                    removal: removal,
                    offered: hovering
                )
            }
        }
        .frame(
            width: ImportMappingColumns.action,
            height: ImportMappingColumns.action
        )
    }

    private func removal(
        _ track: BridgeRawTrackEdit
    ) -> ImportMappingRowRemoval? {
        guard sourceFileEditsAllowed else { return nil }
        return ImportMappingRowRemoval(
            label: coreString("ui.import.slots.drop"),
            help: coreString("ui.import.slots.remove_help")
        ) {
            actions.drop(track.id)
        }
    }

    @ViewBuilder
    private func chooseFileButtons(_ track: BridgeRawTrackEdit) -> some View {
        ForEach(audioChoices) { choice in
            Button {
                actions.chooseFile(track.id, choice.audio)
            } label: {
                Text(verbatim: choice.label)
            }
        }
    }

}
