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
    /// Extracted identifying signals by their source file. The row for that
    /// file carries the chip independently of the selected pressing.
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
        VStack(alignment: .leading, spacing: 18) {
            section(
                title: coreString("ui.import.mapping.tracks_title"),
                trailing: table.reconciliation
                    .flatMap(bridgeSlotReconciliationText)
            ) {
                trackHeaderRow
                ForEach(table.rows, id: \.rowId) { row in
                    trackBody(of: row)
                }
            }
            if !table.keptRows.isEmpty {
                section(title: coreString("ui.import.mapping.files_title")) {
                    fileHeaderRow
                    ForEach(table.keptRows, id: \.rowId) { row in
                        fileBody(of: row)
                    }
                }
            }
        }
    }

    /// One titled card of rows. A pane too narrow for the columns scrolls
    /// sideways rather than squeezing a column past the point it says
    /// anything, and both sections scroll as one so their columns stay aligned.
    @ViewBuilder
    private func section<Rows: View>(
        title: String,
        trailing: String? = nil,
        @ViewBuilder rows: () -> Rows
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: title, trailing: trailing)
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    rows()
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

    // MARK: - Tracks

    /// The rows that become tracks. A sheet heads the run of slices it carves;
    /// the slices are tracks like any other and carry no sheet controls of
    /// their own.
    @ViewBuilder
    private func trackBody(of row: BridgeMappingRow) -> some View {
        switch row {
        case .unit(let unit):
            if unit.isTrack {
                trackRow(unit)
            }
        case .sheet(let sheet, let entries):
            ImportMappingSheetRow(
                sheet: sheet,
                columns: columns.tracks,
                options: bindingOptions[sheet.sheetId],
                evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
                actions: actions,
            )
            .rowChrome()
            ForEach(entries.filter(\.isTrack), id: \.rowId, content: trackRow)
        case .directory:
            EmptyView()
        }
    }

    private func trackRow(_ unit: BridgeMappingUnit) -> some View {
        ImportMappingTrackRow(
            unit: unit,
            columns: columns.tracks,
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

    private var trackHeaderRow: some View {
        headerRow {
            eyebrow("ui.import.mapping.column.source")
                .frame(width: columns.tracks.source, alignment: .leading)
            FormEyebrow(text: Text(verbatim: "#"))
                .frame(
                    width: ImportMappingColumns.position,
                    alignment: .leading
                )
            eyebrow("ui.import.mapping.column.title")
                .frame(width: columns.tracks.title, alignment: .leading)
            eyebrow("ui.import.mapping.column.artist")
                .frame(width: columns.tracks.artist, alignment: .leading)
            eyebrow("ui.import.slots.column.length")
                .frame(width: ImportMappingColumns.length, alignment: .trailing)
        }
    }

    // MARK: - Files

    /// The rows carried with the release that are not its tracks. Being listed
    /// here with a role is the whole statement — there is no sentence saying
    /// they are kept, because the section they are in says it.
    @ViewBuilder
    private func fileBody(of row: BridgeMappingRow) -> some View {
        switch row {
        case .unit(let unit):
            ImportMappingFileRow(
                unit: unit,
                columns: columns.files,
                previewingPath: previewingPath,
                evidence: evidenceFor(unit),
                actions: actions,
            )
            .rowChrome()
        case .directory(let directory):
            ImportMappingDirectoryRow(
                directory: directory,
                columns: columns.files
            )
            .rowChrome()
        case .sheet:
            EmptyView()
        }
    }

    private var fileHeaderRow: some View {
        headerRow {
            eyebrow("ui.import.mapping.column.name")
                .frame(width: columns.files.name, alignment: .leading)
        }
    }

    // MARK: - Shared chrome

    private func eyebrow(_ key: String) -> some View {
        FormEyebrow(text: Text(verbatim: coreString(key)))
    }

    private func headerRow<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            content()
        }
        .padding(.horizontal, ImportMappingColumns.rowPadding)
        .padding(.vertical, 8)
        .background(Theme.surfaceElevated)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
        }
    }

    private func evidenceFor(_ unit: BridgeMappingUnit) -> [BridgeFileEvidence]
    {
        guard case .file(let file) = unit.source else { return [] }
        return ImportEvidence.of(file.fileId, in: evidence)
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

/// The widths the mapping table resolves for one pane.
///
/// Tracks have five columns. Files have one: its Name cell owns the whole inner
/// table, including the inline size, evidence badges, and any actionable role
/// control. Keeping those section shapes explicit prevents hidden Files
/// columns from reserving width for values the section does not show.
///
/// Source leads because the file is the origin of every mapping row. `#` and
/// Length are fixed — a track number and a duration have a known size and
/// squeezing them says nothing. Source, Title, and Artist give up width in
/// proportion as the pane narrows, each down to a floor below which it stops
/// being a column and starts being an ellipsis.
struct ImportMappingColumns {
    struct Tracks {
        let source: CGFloat
        let title: CGFloat
        let artist: CGFloat

        /// The mapped track fields a track-sheet heading replaces. Its Source
        /// cell still owns the descriptor and audio association, while this
        /// span owns the disc assignment that shapes the resulting tracks.
        var mappedTrack: CGFloat {
            ImportMappingColumns.position + title + artist
                + ImportMappingColumns.spacing * 2
        }

        var positionOrigin: CGFloat {
            source + ImportMappingColumns.spacing
        }

        var titleOrigin: CGFloat {
            positionOrigin + ImportMappingColumns.position
                + ImportMappingColumns.spacing
        }

        var artistOrigin: CGFloat {
            titleOrigin + title + ImportMappingColumns.spacing
        }

        var lengthOrigin: CGFloat {
            artistOrigin + artist + ImportMappingColumns.spacing
        }
    }

    struct Files {
        /// The Files section's only column: the whole inner table width.
        let name: CGFloat
    }

    let tracks: Tracks
    let files: Files

    static let position: CGFloat = 34
    static let length: CGFloat = 64
    static let spacing: CGFloat = 10
    static let rowPadding: CGFloat = 14

    private static let idealTitle: CGFloat = 220
    private static let floorTitle: CGFloat = 96
    private static let idealArtist: CGFloat = 180
    private static let floorArtist: CGFloat = 72
    private static let idealSource: CGFloat = 260
    private static let floorSource: CGFloat = 140

    private static let chrome: CGFloat = rowPadding * 2 + spacing * 4
    private static let rigid: CGFloat = position + length

    static let idealTableWidth: CGFloat =
        idealTitle + idealArtist + idealSource + rigid + chrome

    static let minimumTableWidth: CGFloat =
        floorTitle + floorArtist + floorSource + rigid + chrome

    static func resolved(tableWidth: CGFloat) -> ImportMappingColumns {
        let width = max(tableWidth, minimumTableWidth)
        let given =
            width < idealTableWidth
            ? (idealTableWidth - width)
                / ((idealTitle - floorTitle) + (idealArtist - floorArtist)
                    + (idealSource - floorSource))
            : 0
        let title = shrunk(idealTitle, to: floorTitle, by: given)
        let artist = shrunk(idealArtist, to: floorArtist, by: given)
        return ImportMappingColumns(
            tracks: Tracks(
                // What the others leave. Stating the leading flexible share as
                // the remainder keeps all five columns summing to the table's
                // width at every size.
                source: max(
                    floorSource,
                    width - rigid - chrome - title - artist
                ),
                title: title,
                artist: artist
            ),
            files: Files(name: width - rowPadding * 2)
        )
    }

    private static func shrunk(
        _ ideal: CGFloat,
        to floor: CGFloat,
        by given: CGFloat
    ) -> CGFloat {
        max(floor, ideal - (ideal - floor) * given)
    }
}

/// A collapsed directory as the one Files row core decided it should be: its
/// path and aggregate size in the section's one Name column.
struct ImportMappingDirectoryRow: View {
    let directory: BridgeCollapsedDirectory
    let columns: ImportMappingColumns.Files

    var body: some View {
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
        .frame(width: columns.name, alignment: .leading)
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
