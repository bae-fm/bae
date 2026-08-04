import BaeKit
import SwiftUI

/// Section 2 of the mapping pane: every source unit the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet heads the group of entries it
/// carves; the folder's images are one gallery row; a collapsed directory is
/// one row, because the roles of fourteen rip logs are one fact.
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
                ForEach(table.rows, id: \.rowId) { row in
                    body(of: row)
                }
            }
            .formGroupCard()
        }
    }

    @ViewBuilder
    private func body(of row: BridgeMappingRow) -> some View {
        switch row {
        case .unit(let unit):
            unitRow(unit)
        case .sheet(let sheet, let entries):
            ImportMappingSheetRow(
                sheet: sheet,
                options: bindingOptions[sheet.sheetId],
                actions: actions,
            )
            .rowChrome()
            ForEach(entries, id: \.rowId, content: unitRow)
        case .images(let images):
            ImportMappingImagesRow(images: images, actions: actions)
                .rowChrome()
        case .directory(let directory):
            ImportMappingDirectoryRow(directory: directory)
                .rowChrome()
        }
    }

    /// One row for a source unit. The row auditioning its audio is accented;
    /// every other row is plain, and a separator is what tells two of them
    /// apart.
    private func unitRow(_ unit: BridgeMappingUnit) -> some View {
        ImportMappingRowView(
            unit: unit,
            audioChoices: table.audioChoices,
            previewingPath: previewingPath,
            actions: actions,
        )
        .rowChrome(
            background: unit.source.audioPath == previewingPath
                ? Theme.accentSoft : .clear
        )
    }

    private var headerRow: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
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
            .frame(width: ImportMappingColumns.title, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(width: ImportMappingColumns.artist, alignment: .leading)
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.slots.column.length")
                )
            )
            .frame(width: ImportMappingColumns.length, alignment: .trailing)
            Spacer().frame(width: ImportMappingColumns.actions)
        }
        .padding(.horizontal, ImportMappingColumns.rowPadding)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
        }
    }
}

extension View {
    /// What every row of the mapping table sits in: one leading edge, one
    /// height, and a separator over it. No striping — the columns are what a
    /// reader follows across a row, and a tinted band under half of them is a
    /// second, competing grouping.
    fileprivate func rowChrome(background: Color = .clear) -> some View {
        padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.vertical, 6)
            .frame(minHeight: 40)
            .background(background)
            .overlay(alignment: .top) {
                Rectangle()
                    .fill(.white.opacity(0.07))
                    .frame(height: 1)
            }
    }
}

/// The table's columns, shared by its header and every one of its rows.
///
/// Every column but the source is a fixed width, so a header sits over its own
/// column's content on every row: a row negotiating its own widths against its
/// own content is what puts one row's length under another row's role. The
/// source column takes whatever is left, because a file name is the one thing
/// here with no length worth assuming.
enum ImportMappingColumns {
    static let role: CGFloat = 118
    static let position: CGFloat = 34
    static let title: CGFloat = 220
    static let artist: CGFloat = 180
    static let length: CGFloat = 64
    static let actions: CGFloat = 118
    /// The gap between two columns.
    static let spacing: CGFloat = 10
    /// The one leading edge every row starts at.
    static let rowPadding: CGFloat = 14
    /// The length and actions columns with the gap between them — what a row
    /// with no track to edit leaves empty at the end.
    static var trailingColumns: CGFloat { length + actions + spacing }
}

/// A directory whose files all do the same job, as the one row core decided it
/// should be. Each fact under the header it belongs to: the directory and its
/// size where a file's name and size go, what it holds where a role goes, and
/// what becomes of it where every other row says so.
struct ImportMappingDirectoryRow: View {
    let directory: BridgeCollapsedDirectory

    var body: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                Text(directory.dirPrefix)
                    .font(.system(size: 12, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(
                    Int64(directory.totalSize)
                        .formatted(.byteCount(style: .file))
                )
                .font(.caption)
                .foregroundStyle(.tertiary)
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Text(
                bridgeFileRowKindText(directory.kind, count: directory.count)
            )
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: ImportMappingColumns.role, alignment: .leading)
            Spacer().frame(width: ImportMappingColumns.position)
            Text(coreString("ui.import.becomes.kept"))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: ImportMappingColumns.title, alignment: .leading)
            Spacer().frame(width: ImportMappingColumns.artist)
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
        case .images: "images"
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
