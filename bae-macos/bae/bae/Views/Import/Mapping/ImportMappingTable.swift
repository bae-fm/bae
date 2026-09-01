import BaeKit
import SwiftUI

/// Section 2 of the mapping pane: every source unit the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet describes how playable rows are
/// carved, so its source controls head the exact rows it owns. A collapsed
/// directory is one row, because the roles of fourteen rip logs are one fact.
struct ImportMappingTable: View {
    let table: BridgeMappingTable
    /// What each track sheet may be bound to, by the sheet's file id. Core
    /// probes to decide, so the table is handed the answer: a sheet with no
    /// offer yet shows no picker.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    /// The source window currently auditioning, if any — its row is accented.
    let previewingTarget: BridgePreviewTarget?
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
    @State
    var artistFillSelection: ArtistFillSelection?
    @State
    var artistCellFrames: [String: CGRect] = [:]

    let artistFillCoordinateSpace = "ImportMappingTable.artistFill"

    private var tableWidth: CGFloat {
        max(paneWidth, ImportMappingColumns.minimumTableWidth)
    }

    private var columns: ImportMappingColumns {
        ImportMappingColumns.resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            tracksSection
            if !table.files.isEmpty {
                section(title: coreString("ui.import.mapping.files_title")) {
                    fileHeaderRow
                    ForEach(table.files, id: \.rowId) { row in
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
        @ViewBuilder rows: () -> Rows
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: title)
            ScrollView(.horizontal) {
                rowStack(rows)
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
            .onGeometryChange(for: CGFloat.self) { geo in
                geo.size.width
            } action: {
                paneWidth = $0
            }
        }
    }

    private func rowStack<Rows: View>(
        @ViewBuilder _ rows: () -> Rows
    ) -> some View {
        VStack(spacing: 0) {
            rows()
        }
        .frame(width: tableWidth, alignment: .leading)
        .formGroupCard()
    }

    private func artistFillRows<Rows: View>(
        @ViewBuilder _ rows: () -> Rows
    ) -> some View {
        rowStack(rows)
            .coordinateSpace(name: artistFillCoordinateSpace)
            .onPreferenceChange(ArtistCellFramePreferenceKey.self) {
                artistCellFrames = $0
            }
            .overlay(alignment: .topLeading) {
                artistFillOverlay
            }
    }

    // MARK: - Tracks

    private var tracksSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(
                title: coreString("ui.import.mapping.tracks_title"),
                trailing: table.reconciliation
                    .flatMap(bridgeSlotReconciliationText)
            )
            ScrollView(.horizontal) {
                artistFillRows {
                    trackHeaderRow
                    ForEach(
                        table.trackGroups,
                        id: \.rowId,
                        content: trackGroup
                    )
                }
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
    private func trackGroup(_ group: BridgeMappingTrackGroup) -> some View {
        switch group {
        case .unit(let unit):
            trackRow(unit)
        case .sheet(let sheet, let entries):
            sheetRow(sheet)
            ForEach(entries, id: \.rowId, content: trackRow)
        }
    }

    private func sheetRow(_ sheet: BridgeSheetGroup) -> some View {
        ImportMappingSheetRow(
            sheet: sheet,
            options: bindingOptions[sheet.sheetId],
            evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
            actions: actions,
        )
        .sheetGroupHeaderChrome()
    }

    private func trackRow(_ unit: BridgeMappingUnit) -> some View {
        ImportMappingTrackRow(
            unit: unit,
            columns: columns.tracks,
            audioChoices: table.audioChoices,
            previewingTarget: previewingTarget,
            evidence: evidenceFor(unit),
            actions: actions,
            artistFillCoordinateSpace: artistFillCoordinateSpace,
            onSelectArtist: selectArtist,
        )
        .rowChrome(
            background: unit.source.previewTarget == previewingTarget
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
    private func fileBody(of row: BridgeMappingFileRow) -> some View {
        switch row {
        case .file(let file):
            ImportMappingFileRow(
                file: file,
                columns: columns.files,
                previewingTarget: previewingTarget,
                evidence: ImportEvidence.of(file.fileId, in: evidence),
                actions: actions,
            )
            .rowChrome()
        case .sheet(let sheet):
            ImportMappingSheetRow(
                sheet: sheet,
                options: bindingOptions[sheet.sheetId],
                evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
                actions: actions,
                fileColumns: columns.files,
            )
            .rowChrome()
        case .directory(let directory):
            ImportMappingDirectoryRow(
                directory: directory,
                columns: columns.files
            )
            .rowChrome()
        }
    }

    private var fileHeaderRow: some View {
        headerRow {
            eyebrow("ui.import.mapping.column.name")
                .frame(width: columns.files.name, alignment: .leading)
            FormEyebrow(text: Text("Size"))
                .frame(width: columns.files.size, alignment: .trailing)
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

    /// A sheet is the heading for the track rows immediately below it.
    fileprivate func sheetGroupHeaderChrome() -> some View {
        padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.vertical, 8)
            .frame(minHeight: 44)
            .background(Theme.surfaceElevated)
            .overlay(alignment: .leading) {
                Rectangle().fill(Theme.accent).frame(width: 3)
            }
            .overlay(alignment: .top) {
                Rectangle().fill(.white.opacity(0.13)).frame(height: 1)
            }
    }
}

/// The widths the mapping table resolves for one pane.
///
/// Tracks have five columns. Files have a flexible Name and fixed Size column.
/// Keeping those section shapes explicit prevents track-only columns from
/// reserving width for values the Files section does not show.
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
    }

    struct Files {
        let name: CGFloat
        let size: CGFloat
    }

    let tracks: Tracks
    let files: Files

    static let position: CGFloat = 34
    static let length: CGFloat = 88
    static let fileSize: CGFloat = 64
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
            files: Files(
                name: width - rowPadding * 2 - spacing - fileSize,
                size: fileSize
            )
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
/// path and aggregate size in the section's Name and Size columns.
struct ImportMappingDirectoryRow: View {
    let directory: BridgeCollapsedDirectory
    let columns: ImportMappingColumns.Files

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
                Spacer(minLength: 0)
            }
            .frame(width: columns.name, alignment: .leading)
            Text(
                Int64(directory.totalSize)
                    .formatted(.byteCount(style: .file))
            )
            .font(.caption)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .frame(width: columns.size, alignment: .trailing)
        }
    }
}

extension BridgeMappingTrackGroup {
    /// This group's identity in the table. A file and a sheet are named by what
    /// they are; a row the release names with nothing behind it is named by the
    /// track it commits, which core makes unique across the table.
    var rowId: String {
        switch self {
        case .unit(let unit): unit.rowId
        case .sheet(let sheet, _): "sheet:\(sheet.sheetId)"
        }
    }
}

extension BridgeMappingFileRow {
    var rowId: String {
        switch self {
        case .file(let file): "file:\(file.fileId)"
        case .sheet(let sheet): "sheet:\(sheet.sheetId)"
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
