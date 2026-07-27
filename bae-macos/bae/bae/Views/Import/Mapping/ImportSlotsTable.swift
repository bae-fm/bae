import BaeKit
import SwiftUI

/// Zone 3 of the mapping pane: one row per track slot, with the reconciliation
/// line above it.
///
/// The line tallies the folder's audio against the source's tracklist and names
/// the disagreement. It is a statement — nothing here gates the commit.
struct ImportSlotsTable: View {
    let rows: [ImportSlotRow]
    /// Every audio unit the folder offers — what "Choose file…" picks from.
    let audioChoices: [BridgeSlotFile]
    /// The tally core computed; `nil` when no release is picked and there is
    /// nothing to reconcile the folder against.
    let reconciliation: BridgeSlotReconciliation?
    /// The path currently auditioning, if any — the row playing it is accented.
    let previewingPath: String?
    @Binding
    var values: BridgeRawReleaseEdit
    let actions: ImportSlotActions

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(
                title: coreString("ui.import.slots.title"),
                trailing: reconciliation.map(bridgeSlotReconciliationText)
            )
            VStack(spacing: 0) {
                headerRow
                ForEach(rows) { row in
                    ImportSlotRowView(
                        row: row,
                        audioChoices: audioChoices,
                        previewingPath: previewingPath,
                        track: $values.tracks[row.index],
                        actions: actions,
                    )
                    .padding(.horizontal, 14)
                    .frame(minHeight: 40)
                    .background(rowBackground(row))
                }
            }
            .formGroupCard()
        }
    }

    /// The playing row is accented; the rest alternate so a long table stays
    /// readable across its columns.
    private func rowBackground(_ row: ImportSlotRow) -> Color {
        if let path = row.file?.localPath, path == previewingPath {
            return Theme.accentSoft
        }
        return row.index.isMultiple(of: 2) ? .clear : .white.opacity(0.02)
    }

    private var headerRow: some View {
        HStack(spacing: 10) {
            FormEyebrow(text: Text(verbatim: "#"))
                .frame(width: ImportSlotColumns.position, alignment: .leading)
            FormEyebrow(
                text: Text(verbatim: coreString("ui.import.slots.column.file"))
            )
            .frame(width: ImportSlotColumns.file, alignment: .leading)
            Spacer().frame(width: ImportSlotColumns.link)
            FormEyebrow(text: Text("Title"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.slots.column.length")
                )
            )
            .frame(width: ImportSlotColumns.length, alignment: .trailing)
            Spacer().frame(width: ImportSlotColumns.actions)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
        }
    }
}

/// Column widths shared by the slot table's header and its rows, so the two
/// never disagree.
enum ImportSlotColumns {
    static let position: CGFloat = 44
    static let file: CGFloat = 196
    static let link: CGFloat = 16
    static let length: CGFloat = 76
    static let actions: CGFloat = 168
}
