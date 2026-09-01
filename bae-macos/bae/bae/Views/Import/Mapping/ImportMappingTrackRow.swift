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
    let unit: BridgeMappingUnit
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ImportMappingColumns.Tracks
    /// Every audio unit the folder offers — what a row with nothing behind it
    /// is offered to point at.
    let audioChoices: [ImportAudioChoice]
    let previewingTarget: BridgePreviewTarget?
    /// Identifying signals extracted from this row's file. Empty for every
    /// other row.
    var evidence: [BridgeFileEvidence]
    let actions: ImportMappingActions
    var artistFillCoordinateSpace: String?
    var onSelectArtist: (String) -> Void = { _ in }

    @State
    private var hovering = false

    /// Whether the folder and the release disagree about how long this row
    /// runs. Core decides how far apart is far enough — it is a judgement about
    /// how much two rips of one track may legitimately differ, and the other
    /// desktop surface has to reach the same answer.
    private var lengthsDiverge: Bool {
        unit.durationsDiverge
    }

    /// The track this row writes, where a release has named one.
    private var track: BridgeRawTrackEdit? {
        if case .track(let track, _, _) = unit.becomes { return track }
        return nil
    }

    /// A row with no file behind it is the one that has to be answered, so its
    /// picker does not wait to be hovered.
    private var needsAnswer: Bool {
        if case .missing = unit.source { return true }
        return track?.file == nil
    }

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            sourceCell
            Text(position)
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(
                    width: ImportMappingColumns.position,
                    alignment: .leading
                )
            titleCell
            artistCell
            Text(unit.displayedDuration)
                .font(.system(size: 12))
                .monospacedDigit()
                .accessibilityLabel(
                    coreString("ui.import.slots.column.length")
                )
                .accessibilityValue(unit.displayedDuration)
                .foregroundStyle(
                    lengthsDiverge
                        ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary)
                )
                .help(lengthsDiverge ? String(localized: "Lengths differ") : "")
                .frame(
                    width: ImportMappingColumns.length,
                    alignment: .trailing
                )
            removalCell
        }
        // The whole row is the hover shape, gaps included. Hover follows
        // hit-testing, and a stack's empty space is not hit-testable on its
        // own — without this, the pointer crossing a gap on its way to the
        // removal X ends the hover that shows the X.
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu {
            if let track, !audioChoices.isEmpty {
                chooseFileButtons(track)
            }
        }
    }

    private var position: String {
        if case .track(_, let position, _) = unit.becomes {
            return position ?? ""
        }
        return ""
    }

    /// The title, editable where there is a track to edit. Before a release is
    /// picked there is none, and the cell says what the row is waiting for.
    @ViewBuilder
    private var titleCell: some View {
        if let track {
            // The field's chrome fills the column, so its text sits an
            // inline chrome-pad inside it; the Title header carries the same
            // inset, keeping the two aligned without the chrome spilling into
            // the neighbouring column.
            CommittedTextField(
                placeholder: coreString("ui.import.slots.untitled"),
                value: track.title,
                chrome: .inline,
                onCommit: { commit(track, \.title, $0) },
            )
            .frame(width: columns.title)
        }
        else {
            Text(coreString("ui.import.becomes.awaiting_pick"))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: columns.title, alignment: .leading)
        }
    }

    @ViewBuilder
    private var artistCell: some View {
        if let track {
            ArtistAssignmentsField(
                assignments: explicitArtists(track.artistAssignments),
                placeholder: coreString("ui.import.mapping.column.artist"),
                inheritsAlbumArtists: inheritsAlbumArtists(
                    track.artistAssignments
                ),
                onUseAlbumArtists: {
                    commitArtists(track, .albumArtists)
                },
                onChange: {
                    commitArtists(track, .explicit(assignments: $0))
                },
            )
            .modifier(FieldChrome(focused: false, style: .inline))
            .frame(width: columns.artist)
            .simultaneousGesture(
                TapGesture().onEnded { onSelectArtist(track.id) }
            )
            .background {
                if let artistFillCoordinateSpace {
                    GeometryReader { geometry in
                        Color.clear.preference(
                            key: ArtistCellFramePreferenceKey.self,
                            value: [
                                track.id: geometry.frame(
                                    in: .named(artistFillCoordinateSpace)
                                )
                            ]
                        )
                    }
                }
            }
        }
        else {
            Spacer().frame(width: columns.artist)
        }
    }

    /// What the folder offers for this track, and — while the row has nothing
    /// — the picker that answers it.
    private var sourceCell: some View {
        ImportMappingSourceCell(
            source: unit.source,
            previewingTarget: previewingTarget,
            evidence: evidence,
            showsFileSize: true,
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
                    if let target = unit.source.previewTarget {
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
        if case .file(let file) = unit.source {
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

    /// Store one field of this row's track. A row is edited as a unit, so the
    /// whole row goes — the field the user left is the one that changed.
    private func commit(
        _ track: BridgeRawTrackEdit,
        _ path: WritableKeyPath<BridgeRawTrackEdit, String>,
        _ value: String
    ) {
        var edited = track
        edited[keyPath: path] = value
        actions.editTrack(edited)
    }

    private func commitArtists(
        _ track: BridgeRawTrackEdit,
        _ assignments: BridgeTrackArtistAssignments
    ) {
        var edited = track
        edited.artistAssignments = assignments
        actions.editTrack(edited)
    }

    private func explicitArtists(
        _ assignments: BridgeTrackArtistAssignments
    ) -> [BridgeArtistAssignment] {
        switch assignments {
        case .albumArtists: []
        case .explicit(let artists): artists
        }
    }

    private func inheritsAlbumArtists(
        _ assignments: BridgeTrackArtistAssignments
    ) -> Bool {
        if case .albumArtists = assignments { return true }
        return false
    }
}
