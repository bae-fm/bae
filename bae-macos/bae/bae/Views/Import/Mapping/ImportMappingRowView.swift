import BaeKit
import SwiftUI

/// One row of the mapping table: what the folder offers on the left, and what
/// committing makes of it on the right.
///
/// The two lengths are the point of a track row. Counting cannot see a pairing
/// that is complete but wrong — thirteen files against thirteen tracks in the
/// wrong order counts perfectly — and reading the file's own length against the
/// release's is what catches it.
struct ImportMappingRowView: View {
    let unit: BridgeMappingUnit
    /// Whether this row is one of a track sheet's entries, which sit inside
    /// their group header.
    let indented: Bool
    /// Every audio unit the folder offers — what a row with nothing behind it
    /// is offered to point at.
    let audioChoices: [ImportAudioChoice]
    let previewingPath: String?
    let actions: ImportMappingActions

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

    var body: some View {
        HStack(spacing: 10) {
            ImportMappingSourceCell(
                source: unit.source,
                previewingPath: previewingPath,
                lengthsDiverge: lengthsDiverge,
                actions: actions,
            )
            .padding(
                .leading,
                indented ? ImportMappingColumns.entryIndent : 0
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            ImportMappingRoleCell(source: unit.source, actions: actions)
                .frame(width: ImportMappingColumns.role, alignment: .leading)
            becomesCells
        }
    }

    /// What committing makes of this row. A track is edited in place; anything
    /// else states what it becomes and has nothing to edit.
    @ViewBuilder
    private var becomesCells: some View {
        switch unit.becomes {
        case .track(let track, let position, let sourceMs):
            Text(position ?? "")
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(
                    width: ImportMappingColumns.position,
                    alignment: .leading
                )
            MetadataField(
                placeholder: coreString("ui.import.slots.untitled"),
                text: field(of: track, \.title),
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            MetadataField(
                placeholder: String(localized: "Artist"),
                text: field(of: track, \.artistText),
                boxed: false,
            )
            .frame(maxWidth: .infinity)
            Text(importDurationText(sourceMs))
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
            rowActions(track)
                .frame(
                    width: ImportMappingColumns.actions,
                    alignment: .trailing
                )
        case .kept:
            becomesText(coreString("ui.import.becomes.kept"))
        case .awaitingPick:
            becomesText(coreString("ui.import.becomes.awaiting_pick"))
        }
    }

    /// What a row that commits no track becomes, laid out over the same columns
    /// the editable rows use so the two line up: the position column, the
    /// statement across the title field, and the space the artist field, the
    /// length and the actions leave.
    @ViewBuilder
    private func becomesText(_ text: String) -> some View {
        Spacer().frame(width: ImportMappingColumns.position)
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
        Spacer().frame(maxWidth: .infinity)
        Spacer().frame(width: ImportMappingColumns.trailingColumns)
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

    /// One field of this row's track, writing the edited track back onto the
    /// row that commits it.
    private func field(
        of track: BridgeRawTrackEdit,
        _ path: WritableKeyPath<BridgeRawTrackEdit, String>
    ) -> Binding<String> {
        Binding(
            get: { track[keyPath: path] },
            set: { newValue in
                var edited = track
                edited[keyPath: path] = newValue
                actions.editTrack(edited)
            },
        )
    }
}
