import BaeKit
import SwiftUI

/// One of the release's tracks: the file behind it, its number, what it will be
/// called, who it is by, and how long it runs.
///
/// The title and artist are edited in place — this is the release being
/// written, not a report of it. The control that re-points the row at another
/// file is not a column: it appears over the Source cell when the pointer is
/// on the row, and stays put on a row that has no file, which is the one row
/// that has to be answered.
struct ImportMappingTrackRow: View {
    let unit: BridgeMappingUnit
    /// The widths the table resolved for this pane, so the row's cells land
    /// under the header's.
    let columns: ImportMappingColumns.Tracks
    /// Every audio unit the folder offers — what a row with nothing behind it
    /// is offered to point at.
    let audioChoices: [ImportAudioChoice]
    let previewingPath: String?
    /// Whether this row's audio has not been read yet.
    var isMeasuring: Bool = false
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
        bridgeLengthsDisagree(
            probedMs: unit.source.durationMs,
            sourceMs: unit.sourceDurationMs
        )
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
            Text(importDurationText(unit.sourceDurationMs))
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(
                    lengthsDiverge
                        ? AnyShapeStyle(.orange) : AnyShapeStyle(.primary)
                )
                .help(lengthsDiverge ? String(localized: "Lengths differ") : "")
                .frame(
                    width: ImportMappingColumns.length,
                    alignment: .trailing
                )
        }
        .onHover { hovering = $0 }
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
            CommittedTextField(
                placeholder: coreString("ui.import.slots.untitled"),
                value: track.title,
                boxed: false,
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

    /// What the folder offers for this track, and — while the pointer is here,
    /// or while the row has nothing — the controls that change it.
    private var sourceCell: some View {
        ImportMappingSourceCell(
            source: unit.source,
            previewingPath: previewingPath,
            lengthsDiverge: lengthsDiverge,
            isMeasuring: isMeasuring,
            evidence: evidence,
            actions: actions,
        )
        .frame(width: columns.source, alignment: .leading)
        .overlay(alignment: .trailing) {
            if hovering || needsAnswer, let track {
                rowActions(track)
                    .padding(.leading, 8)
                    .background(Theme.surfaceElevated.opacity(0.94))
            }
        }
    }

    /// Pick the audio this row writes, and the one action that belongs to the
    /// row's own disagreement — Exclude for audio the release does not name,
    /// Drop for a track this folder has nothing for.
    ///
    /// Re-pairing is this menu, not a drag. A drag needs a second hit target
    /// and a second interaction design per toolkit, has no keyboard or
    /// accessibility path, and buys nothing over picking from the folder's
    /// audio by name — which is what re-pointing a row and swapping two rows
    /// both come down to.
    @ViewBuilder
    private func rowActions(_ track: BridgeRawTrackEdit) -> some View {
        HStack(spacing: 8) {
            chooseFileMenu(track)
            if unit.sourcePosition == nil, case .file(let file) = unit.source {
                Button(coreString("ui.import.slots.exclude")) {
                    actions.exclude(file.fileId)
                }
                .buttonStyle(.link)
                .font(.system(size: 11.5))
            }
            else if track.file == nil {
                Button(coreString("ui.import.slots.drop")) {
                    actions.drop(track.id)
                }
                .buttonStyle(.link)
                .font(.system(size: 11.5))
            }
        }
        .fixedSize()
    }

    @ViewBuilder
    private func chooseFileMenu(_ track: BridgeRawTrackEdit) -> some View {
        if !audioChoices.isEmpty {
            Menu {
                ForEach(audioChoices) { choice in
                    Button {
                        actions.chooseFile(track.id, choice.audio)
                    } label: {
                        Text(verbatim: choice.label)
                    }
                }
            } label: {
                Text(coreString("ui.import.slots.choose_file"))
                    .font(.system(size: 11.5))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
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
