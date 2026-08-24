import BaeKit
import SwiftUI

/// Section 2 of the mapping pane: every source unit the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet heads the group of entries it
/// carves; a collapsed directory is one row, because the roles of fourteen rip
/// logs are one fact.
struct ImportMappingTable: View {
    let table: BridgeMappingTable
    /// What each track sheet may be bound to, by the sheet's file id. Core
    /// probes to decide, so the table is handed the answer: a sheet with no
    /// offer yet shows no picker.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The path currently auditioning, if any — the row playing it is accented.
    let previewingPath: String?
    /// The audio units nothing has read yet. Their rows say so while the read
    /// runs; every other length on the pane is already stored.
    var unprobed: Set<BridgeAudioFile> = []
    /// What identified the release, by the file each piece was read off. The
    /// row for that file carries the chip.
    var evidence: [BridgeFileEvidence] = []
    let actions: ImportMappingActions

    /// The width the pane leaves the table. The columns are resolved against
    /// it, and the table is laid out at it or at its own minimum, whichever is
    /// wider — so the pane never has more table than it has room for, and the
    /// row never has less than its columns need.
    @State
    private var paneWidth: CGFloat = ImportMappingColumns.minimumTableWidth

    private var tableWidth: CGFloat {
        max(paneWidth, ImportMappingColumns.minimumTableWidth)
    }

    private var columns: ImportMappingColumns {
        ImportMappingColumns.resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(
                title: coreString("ui.import.mapping.title"),
                trailing: table.reconciliation
                    .flatMap(bridgeSlotReconciliationText)
            )
            // A pane too narrow for the columns scrolls the table sideways.
            // The alternative is squeezing a column past the point it says
            // anything — or, as it stood, running the last two off the pane's
            // right edge where there is no way to reach them at all.
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    headerRow
                    ForEach(table.rows, id: \.rowId) { row in
                        body(of: row)
                    }
                }
                .frame(width: tableWidth, alignment: .leading)
                .formGroupCard()
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            .onGeometryChange(for: CGFloat.self) { geo in
                geo.size.width
            } action: {
                paneWidth = $0
            }
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
                columns: columns,
                options: bindingOptions[sheet.sheetId],
                evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
                actions: actions,
            )
            .rowChrome()
            ForEach(entries, id: \.rowId, content: unitRow)
        case .directory(let directory):
            ImportMappingDirectoryRow(
                directory: directory,
                columns: columns
            )
            .rowChrome()
        }
    }

    /// One row for a source unit. The row auditioning its audio is accented;
    /// every other row is plain, and a separator is what tells two of them
    /// apart.
    private func unitRow(_ unit: BridgeMappingUnit) -> some View {
        ImportMappingRowView(
            unit: unit,
            columns: columns,
            audioChoices: table.audioChoices,
            previewingPath: previewingPath,
            isMeasuring: unit.source.audio.map(unprobed.contains) ?? false,
            evidence: evidenceFor(unit),
            actions: actions,
        )
        .rowChrome(
            background: unit.source.audioPath == previewingPath
                ? Theme.accentSoft : .clear
        )
    }

    /// The evidence this row's own file is the source of, if it is any.
    private func evidenceFor(_ unit: BridgeMappingUnit) -> BridgeFileEvidence? {
        guard case .file(let file) = unit.source else { return nil }
        return ImportEvidence.of(file.fileId, in: evidence)
    }

    private var headerRow: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            FormEyebrow(
                text: Text(
                    verbatim: coreString("ui.import.mapping.column.source")
                )
            )
            .frame(width: columns.source, alignment: .leading)
            FormEyebrow(
                text: Text(verbatim: coreString("ui.import.roles.column.role"))
            )
            .frame(width: columns.role, alignment: .leading)
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
            .frame(width: columns.title, alignment: .leading)
            FormEyebrow(text: Text("Artist"))
                .frame(width: columns.artist, alignment: .leading)
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
    /// A cell in the source column: exactly the column's width, and clipped to
    /// it. A file name truncates on its own; what a cell cannot truncate — a
    /// control or a size — stops at the column's edge rather than
    /// running into the role beside it.
    func sourceColumn(_ columns: ImportMappingColumns) -> some View {
        frame(width: columns.source, alignment: .leading)
            .clipped()
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

/// The table's columns, resolved against the width the table has to lay them
/// out in and shared by its header and every one of its rows.
///
/// The widths are the table's, not each row's: a row negotiating its own widths
/// against its own content is what puts one row's length under another row's
/// role.
///
/// One set of widths only holds at one table width, which is the whole reason
/// this is resolved rather than declared. Four columns carry the give: the
/// source and the role, and the two editable fields. At `idealTableWidth` each
/// of them has what it asks for; every point narrower is taken off all four at
/// once, each giving up its share of the shortfall in proportion to how much it
/// has to give, so they arrive at their floors together instead of one
/// collapsing while its neighbour is still roomy. `minimumTableWidth` is where
/// all four sit at their floor; the table is never laid out narrower than that,
/// because a column squeezed past its floor stops saying what it is there to
/// say — the pane scrolls it sideways instead.
///
/// Every column is an exact width, the source included. A cell asked to fit in
/// less than its content wants must truncate inside its column and not push its
/// neighbours along: a row whose width is negotiated with its own cells is a
/// row that can come out wider than the table, which is how the last columns
/// ended up off the pane's right edge.
struct ImportMappingColumns {
    let source: CGFloat
    let role: CGFloat
    let title: CGFloat
    let artist: CGFloat

    /// The position, the length, and the row's own actions. There is no
    /// truncation of a track number, a running time or the two words that *are*
    /// the actions that leaves something still readable, so these three are the
    /// same width at every table width.
    static let position: CGFloat = 34
    static let length: CGFloat = 64
    static let actions: CGFloat = 118
    /// The gap between two columns.
    static let spacing: CGFloat = 10
    /// The one leading edge every row starts at.
    static let rowPadding: CGFloat = 14
    /// The length and actions columns with the gap between them — what a row
    /// with no track to edit leaves empty at the end.
    static var trailingColumns: CGFloat { length + actions + spacing }

    /// What each column that gives asks for, and what it will come down to: a
    /// file name truncated in the middle beside its audition control, a role
    /// word still read as that word, and a field narrow enough to scroll under
    /// the caret but wide enough to read a title in.
    private static let idealSource: CGFloat = 240
    private static let floorSource: CGFloat = 110
    private static let idealRole: CGFloat = 118
    private static let floorRole: CGFloat = 60
    private static let idealTitle: CGFloat = 220
    private static let floorTitle: CGFloat = 96
    private static let idealArtist: CGFloat = 180
    private static let floorArtist: CGFloat = 72

    /// What a row spends on something that is not a column: the leading edge at
    /// each end, and the six gaps between seven columns.
    private static let chrome: CGFloat = rowPadding * 2 + spacing * 6
    /// The three columns that are the same width at every table width.
    private static let rigid: CGFloat = position + length + actions

    /// The width at which every column has what it asks for. Wider than this,
    /// the surplus is the source's and nothing else moves.
    static let idealTableWidth: CGFloat =
        idealSource + idealRole + idealTitle + idealArtist + rigid + chrome

    /// The width at which all four giving columns are at their floor. The table
    /// is laid out at this width even when the pane is narrower, and the pane
    /// scrolls it sideways rather than squeezing a column out of the row.
    static let minimumTableWidth: CGFloat =
        floorSource + floorRole + floorTitle + floorArtist + rigid + chrome

    static func resolved(tableWidth: CGFloat) -> ImportMappingColumns {
        let width = max(tableWidth, minimumTableWidth)
        let given =
            width < idealTableWidth
            ? (idealTableWidth - width)
                / ((idealSource - floorSource) + (idealRole - floorRole)
                    + (idealTitle - floorTitle) + (idealArtist - floorArtist))
            : 0
        let role = shrunk(idealRole, to: floorRole, by: given)
        let title = shrunk(idealTitle, to: floorTitle, by: given)
        let artist = shrunk(idealArtist, to: floorArtist, by: given)
        return ImportMappingColumns(
            // What the other six leave. Stating the source's share as the
            // remainder rather than as its own number is what makes the seven
            // add up to the table exactly at every width: the surplus above
            // `idealTableWidth` lands here, and nowhere else.
            source: width - chrome - rigid - role - title - artist,
            role: role,
            title: title,
            artist: artist
        )
    }

    /// A column `fraction` of the way from what it asks for to what it will
    /// take. Not rounded to whole points: rounding three columns and leaving
    /// the fourth the remainder means widening the table by a point can *narrow*
    /// the source, and a column that walks backwards as the pane is dragged
    /// wider is worse than one sitting on a half-point.
    private static func shrunk(
        _ ideal: CGFloat,
        to floor: CGFloat,
        by fraction: CGFloat
    ) -> CGFloat {
        ideal - (ideal - floor) * fraction
    }
}

/// A directory whose files all do the same job, as the one row core decided it
/// should be. Each fact under the header it belongs to: the directory and its
/// size where a file's name and size go, what it holds where a role goes, and
/// what becomes of it where every other row says so.
struct ImportMappingDirectoryRow: View {
    let directory: BridgeCollapsedDirectory
    let columns: ImportMappingColumns

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
            .sourceColumn(columns)
            Text(
                bridgeFileRowKindText(directory.kind, count: directory.count)
            )
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(width: columns.role, alignment: .leading)
            Spacer().frame(width: ImportMappingColumns.position)
            Text(coreString("ui.import.becomes.kept"))
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .frame(width: columns.title, alignment: .leading)
            Spacer().frame(width: columns.artist)
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
