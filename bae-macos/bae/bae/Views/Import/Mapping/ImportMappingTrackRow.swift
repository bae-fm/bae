import BaeKit
import SwiftUI

/// One of the release's tracks: the file behind it, its number, what it will be
/// called, who it is by, and how long it runs.
///
/// The title and artist are edited in place — this is the release being
/// written, not a report of it. A row without audio shows its file picker in
/// Source. Re-pairing a settled row remains available from its context menu
/// without occupying the table.
struct ImportMappingTrackRow: View {
    let mapping: BridgeTrackMapping
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ReleaseMetadataTrackColumns
    /// Every audio unit the folder offers — what a row with nothing behind it
    /// is offered to point at.
    let audioChoices: [ImportAudioChoice]
    let previewingTarget: BridgePreviewTarget?
    let editingCommands: EditingCommitCommands
    /// Identifying signals extracted from this row's file. Empty for every
    /// other row.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions
    var artistFillCoordinateSpace: String?
    /// Whether the pointer is on this row — where the table shows the artist
    /// fill handle.
    var onArtistFillHover: (Bool) -> Void = { _ in }

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

    /// A row with no file behind it is the one that has to be answered, so its
    /// picker does not wait to be hovered.
    private var needsAnswer: Bool {
        if case .missing = mapping.source { return true }
        return track?.file == nil
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
                    onChange: { actions.editTrack($0) },
                    artistFillCoordinateSpace: artistFillCoordinateSpace
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
            onArtistFillHover($0)
        }
        .contextMenu {
            if let track, !audioChoices.isEmpty {
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

    /// What the folder offers for this track, and — while the row has nothing
    /// — the picker that answers it.
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
        .overlay(alignment: .trailing) {
            if let track, needsAnswer {
                chooseFileMenu(track)
                    .padding(.leading, 8)
                    .background(Theme.surfaceElevated.opacity(0.94))
            }
        }
    }

    /// The one action that belongs to the row's own disagreement, at the far
    /// right where a row is taken out of a list: Exclude for audio the release
    /// does not name, Drop for a track this folder has nothing for. Offered
    /// while the pointer is on the row; a settled row has nothing to offer and
    /// keeps the slot empty so every row ends at the same edge.
    ///
    /// Re-pairing is the context menu, not a drag. A drag needs a second hit
    /// target and a second interaction design per toolkit, has no keyboard or
    /// accessibility path, and buys nothing over picking from the folder's
    /// audio by name — which is what re-pointing a row and swapping two rows
    /// both come down to.
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

    /// What taking this row out means, where it means anything.
    private func removal(
        _ track: BridgeRawTrackEdit
    ) -> ImportMappingRowRemoval? {
        if case .file(let file) = mapping.source {
            return ImportMappingRowRemoval(
                label: coreString("ui.import.slots.exclude"),
                help: coreString("ui.import.slots.exclude_help")
            ) {
                actions.exclude(file.fileId)
            }
        }
        if track.file == nil {
            return ImportMappingRowRemoval(
                label: coreString("ui.import.slots.drop"),
                help: coreString("ui.import.slots.drop_help")
            ) {
                actions.drop(track.id)
            }
        }
        return nil
    }

    @ViewBuilder
    private func chooseFileMenu(_ track: BridgeRawTrackEdit) -> some View {
        if !audioChoices.isEmpty {
            Menu {
                chooseFileButtons(track)
            } label: {
                Text(coreString("ui.import.slots.choose_file"))
                    .font(.system(size: 11.5))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
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
