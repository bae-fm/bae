import BaeKit
import SwiftUI

/// One file carried with the release that is not one of its tracks: a rip log,
/// a text file, audio somebody took out of the tracklist.
///
/// It says its name, how big it is, and the job it has. Nothing says it is
/// kept — it is listed in the import under a heading that says Files, which is
/// the same statement without the sentence.
struct ImportMappingFileRow: View {
    let unit: BridgeMappingUnit
    let columns: ImportMappingColumns
    let previewingPath: String?
    /// What identified the release, where this file is what it was read off —
    /// the rip log a disc ID was computed from wears its chip here.
    var evidence: BridgeFileEvidence?
    let actions: ImportMappingActions

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            ImportMappingSourceCell(
                source: unit.source,
                previewingPath: previewingPath,
                lengthsDiverge: false,
                evidence: evidence,
                actions: actions,
            )
            .frame(width: nameWidth, alignment: .leading)
            Text(importDurationText(unit.source.durationMs))
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(
                    width: ImportMappingColumns.length,
                    alignment: .trailing
                )
            roleCell
                .frame(width: columns.source, alignment: .leading)
        }
    }

    /// The Name cell spans what the Tracks section spends on its number, title
    /// and artist, so a file's name starts where a track's number does and the
    /// two sections read as one table.
    private var nameWidth: CGFloat {
        columns.title + columns.artist + ImportMappingColumns.position
            + ImportMappingColumns.spacing * 2
    }

    /// The job core gave this file, as a chip — or, where the role is the
    /// user's to set, the control that sets it.
    private var roleCell: some View {
        ImportMappingRoleCell(source: unit.source, actions: actions)
    }
}

/// The job one file has, as a chip: what the Role column holds.
struct ImportRoleChip: View {
    let role: BridgeFileRole

    var body: some View {
        Text(coreString(bridgeFileRoleKey(role: role)))
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(Color.secondary.opacity(0.15), in: Capsule())
            .foregroundStyle(.secondary)
            .lineLimit(1)
    }
}
