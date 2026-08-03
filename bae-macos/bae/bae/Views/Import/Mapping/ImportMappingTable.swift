import BaeKit
import SwiftUI

/// Section 2 of the mapping pane: every source unit the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet heads the group of entries it
/// carves; a collapsed directory is one row, because the roles of fourteen
/// scans are one fact.
struct ImportMappingTable: View {
    let table: BridgeMappingTable
    /// What each track sheet may be bound to, by the sheet's file id. Core
    /// probes to decide, so the table is handed the answer: a sheet with no
    /// offer yet shows no picker.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any — the row playing it is accented.
    let previewingPath: String?
    let actions: ImportMappingActions

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(
                title: coreString("ui.import.mapping.title"),
                trailing: table.reconciliation
                    .map(bridgeSlotReconciliationText)
            )
            VStack(spacing: 0) {
                headerRow
                ForEach(Array(table.rows.enumerated()), id: \.element.rowId) {
                    index,
                    row in
                    body(of: row, striped: index.isMultiple(of: 2))
                }
            }
            .formGroupCard()
        }
    }

    @ViewBuilder
    private func body(
        of row: BridgeMappingRow,
        striped: Bool
    ) -> some View {
        switch row {
        case .unit(let unit):
            unitRow(unit, indented: false, striped: striped)
        case .sheet(let sheet, let entries):
            ImportMappingSheetRow(
                sheet: sheet,
                options: bindingOptions[sheet.sheetId],
                actions: actions,
            )
            .padding(.horizontal, 14)
            .frame(minHeight: 40)
            .background(striped ? Color.clear : .white.opacity(0.02))
            ForEach(Array(entries.enumerated()), id: \.element.rowId) {
                offset,
                entry in
                unitRow(
                    entry,
                    indented: true,
                    striped: (offset + (striped ? 1 : 0)).isMultiple(of: 2)
                )
            }
        case .directory(let directory):
            ImportMappingDirectoryRow(directory: directory)
                .padding(.horizontal, 14)
                .frame(minHeight: 40)
                .background(striped ? Color.clear : .white.opacity(0.02))
        }
    }

    private func unitRow(
        _ unit: BridgeMappingUnit,
        indented: Bool,
        striped: Bool
    ) -> some View {
        ImportMappingRowView(
            unit: unit,
            indented: indented,
            audioChoices: table.audioChoices,
            previewingPath: previewingPath,
            actions: actions,
        )
        .padding(.horizontal, 14)
        .frame(minHeight: 40)
        .background(rowBackground(unit, striped: striped))
    }

    /// The playing row is accented; the rest alternate so a long table stays
    /// readable across its columns.
    private func rowBackground(
        _ unit: BridgeMappingUnit,
        striped: Bool
    ) -> Color {
        if let path = unit.source.audioPath, path == previewingPath {
            return Theme.accentSoft
        }
        return striped ? .clear : .white.opacity(0.02)
    }

    private var headerRow: some View {
        HStack(spacing: 10) {
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.mapping.column.source")
                )
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(
                text: Text(verbatim: coreString("ui.import.roles.column.role"))
            )
            .frame(width: ImportMappingColumns.role, alignment: .leading)
            FormEyebrow(text: Text(verbatim: "#"))
                .frame(
                    width: ImportMappingColumns.position,
                    alignment: .leading
                )
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.roles.column.becomes")
                )
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(maxWidth: .infinity, alignment: .leading)
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.slots.column.length")
                )
            )
            .frame(width: ImportMappingColumns.length, alignment: .trailing)
            Spacer().frame(width: ImportMappingColumns.actions)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
        }
    }
}

/// Column widths shared by the mapping table's header and its rows, so the two
/// never disagree.
enum ImportMappingColumns {
    static let role: CGFloat = 118
    static let position: CGFloat = 34
    static let length: CGFloat = 64
    static let actions: CGFloat = 118
    /// How far a track sheet's entries sit inside their group header.
    static let entryIndent: CGFloat = 18
    /// The length and actions columns with the gap between them — what a row
    /// with no track to edit leaves empty at the end.
    static var trailingColumns: CGFloat { length + actions + 10 }
}

/// A directory whose files all do the same job, as the one row core decided it
/// should be: the prefix, what it holds, and the total size.
struct ImportMappingDirectoryRow: View {
    let directory: BridgeCollapsedDirectory

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                Text(directory.dirPrefix)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(
                    verbatim: "\u{2014} "
                        + bridgeFileRowKindText(
                            directory.kind,
                            count: directory.count
                        )
                )
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                Spacer(minLength: 4)
                Text(
                    Int64(directory.totalSize)
                        .formatted(.byteCount(style: .file))
                )
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer().frame(width: ImportMappingColumns.role)
            Spacer().frame(width: ImportMappingColumns.position)
            Text(coreString("ui.import.becomes.kept"))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            Spacer().frame(maxWidth: .infinity)
            Spacer().frame(width: ImportMappingColumns.trailingColumns)
        }
    }
}

extension BridgeMappingRow {
    /// This row's identity in the table. A file and a sheet are named by what
    /// they are; a row the release names with nothing behind it is named by the
    /// track it commits, which core makes unique across the table.
    var rowId: String {
        switch self {
        case .unit(let unit): unit.rowId
        case .sheet(let sheet, _): "sheet:\(sheet.sheetId)"
        case .directory(let directory): "dir:\(directory.dirPrefix)"
        }
    }
}

extension BridgeMappingUnit {
    /// This unit's identity in the table.
    var rowId: String {
        switch source {
        case .file(let file): "file:\(file.fileId)"
        case .sheetEntry(let entry): "entry:\(entry.sheetId):\(entry.index)"
        case .missing: "track:\(track?.id ?? "")"
        }
    }
}
