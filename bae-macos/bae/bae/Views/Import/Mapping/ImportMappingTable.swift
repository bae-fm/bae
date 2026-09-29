import BaeKit
import SwiftUI

/// The mapping pane's table: every source the folder offers on the same row
/// as the track committing makes of it.
struct ImportMappingTable: View {
    let table: BridgeMappingTable
    /// The source window currently auditioning, if any — its row is accented.
    let previewingTarget: BridgePreviewTarget?
    /// Extracted identifying signals by their source file.
    var evidence: [BridgeFileEvidence] = []
    let actions: ImportMappingActions
    let editingCommands: EditingCommitCommands

    /// The width the pane leaves the table; the table is laid out at it or at
    /// its own minimum, whichever is wider.
    @State
    private var paneWidth: CGFloat = ReleaseMetadataTrackColumns
        .minimumTableWidth
    private var tableWidth: CGFloat {
        max(paneWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        ReleaseMetadataTrackColumns.resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.edge) {
            tracksSection
            if !table.files.isEmpty {
                section {
                    fileHeaderRow
                    ForEach(table.files, id: \.rowId) { row in
                        fileBody(of: row)
                    }
                }
            }
        }
    }

    /// One run of rows, scrolling sideways when the pane is too narrow for
    /// its columns.
    @ViewBuilder
    private func section<Rows: View>(
        @ViewBuilder rows: () -> Rows
    ) -> some View {
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

    private func rowStack<Rows: View>(
        @ViewBuilder _ rows: () -> Rows
    ) -> some View {
        VStack(spacing: 0) {
            rows()
        }
        .frame(width: tableWidth, alignment: .leading)
    }

    // MARK: - Tracks

    /// One section per side or disc: track mappings, or one sheet and its
    /// entries.
    private var tracksSection: some View {
        ScrollView(.horizontal) {
            rowStack {
                if table.trackSections.isEmpty {
                    trackHeaderRow
                }
                ForEach(
                    Array(table.trackSections.enumerated()),
                    id: \.offset
                ) { index, section in
                    if !section.sideHeaderText.isEmpty {
                        sideHeader(section.sideHeaderText, index: index)
                    }
                    if case .sheet(let sheet, _) = section.content {
                        sheetCaption(sheet)
                    }
                    if index == 0 {
                        trackHeaderRow
                    }
                    trackSectionRows(section)
                }
            }
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .onGeometryChange(for: CGFloat.self) { geo in
            geo.size.width
        } action: {
            paneWidth = $0
        }
    }

    @ViewBuilder
    private func trackSectionRows(_ section: BridgeMappingTrackSection)
        -> some View
    {
        switch section.content {
        case .tracks(let mappings):
            ForEach(mappings, id: \.rowId, content: trackRow)
        case .sheet(_, let entries):
            ForEach(entries, id: \.rowId, content: trackRow)
        }
    }

    private func sideHeader(_ text: String, index: Int) -> some View {
        Eyebrow(verbatim: text)
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, index == 0 ? ThemeSpace.line : ThemeSpace.edge)
            .padding(.bottom, ThemeSpace.compact)
    }

    private func sheetCaption(_ sheet: BridgeSheetGroup) -> some View {
        sheetCaptionRow(sheet)
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.top, ThemeSpace.line)
            .padding(.bottom, ThemeSpace.related)
    }

    private func sheetCaptionRow(_ sheet: BridgeSheetGroup) -> some View {
        ImportSheetCaptionRow(
            sheet: sheet,
            evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
            showsDiscMenu: true,
            actions: actions,
        )
    }

    private func trackRow(_ mapping: BridgeTrackMapping) -> some View {
        ImportMappingTrackRow(
            mapping: mapping,
            columns: columns,
            previewingTarget: previewingTarget,
            editingCommands: editingCommands,
            evidence: evidenceFor(mapping),
            actions: actions,
        )
        .rowChrome(
            background: mapping.source.previewTarget == previewingTarget
                ? Theme.accentSoft : .clear
        )
    }

    // The leading cell names the section.
    private var trackHeaderRow: some View {
        headerRow {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Eyebrow("Source")
                Spacer(minLength: 0)
            }
            .frame(width: columns.source, alignment: .leading)
            Eyebrow("Track")
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            // Inset to line up with the fields' text.
            eyebrow("ui.import.mapping.column.title")
                .padding(.leading, FieldChrome.inlineHorizontalPadding)
                .frame(width: columns.title, alignment: .leading)
            eyebrow("ui.import.mapping.column.artist")
                .padding(.leading, FieldChrome.inlineHorizontalPadding)
                .frame(width: columns.artist, alignment: .leading)
            eyebrow("ui.import.slots.column.length")
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .trailing
                )
        }
    }

    // MARK: - Files

    /// A row carried with the release that is not one of its tracks.
    @ViewBuilder
    private func fileBody(of row: BridgeMappingFileRow) -> some View {
        switch row {
        case .file(let file):
            ImportMappingFileRow(
                file: file,
                previewingTarget: previewingTarget,
                evidence: ImportEvidence.of(file.fileId, in: evidence),
                actions: actions,
            )
            .rowChrome()
        case .sheet(let sheet):
            sheetCaptionRow(sheet)
                .rowChrome()
        }
    }

    private var fileHeaderRow: some View {
        headerRow {
            eyebrow("ui.import.mapping.files_title")
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: - Shared chrome

    private func eyebrow(_ key: String) -> some View {
        Eyebrow(verbatim: coreString(key))
    }

    private func headerRow<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            content()
        }
        .padding(.horizontal, ImportMappingColumns.rowPadding)
        .padding(.top, ThemeSpace.inline)
        .padding(.bottom, ThemeSpace.compact)
    }

    private func evidenceFor(_ mapping: BridgeTrackMapping)
        -> [BridgeFileEvidence]
    {
        guard case .file(let file) = mapping.source else { return [] }
        return ImportEvidence.of(file.fileId, in: evidence)
    }
}

extension View {
    /// What every row of the mapping table sits in: one leading edge, one
    /// height, and a hairline over it.
    fileprivate func rowChrome(background: Color = .clear) -> some View {
        padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.vertical, ThemeSpace.compact)
            .frame(minHeight: 40)
            .background(background)
            .overlay(alignment: .top) {
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(height: 1)
            }
    }
}

enum ImportMappingColumns {
    static let spacing = ReleaseMetadataTrackColumns.spacing
    static let rowPadding = ReleaseMetadataTrackColumns.rowPadding
}

extension BridgeMappingFileRow {
    var rowId: String {
        switch self {
        case .file(let file): "file:\(file.fileId)"
        case .sheet(let sheet): "sheet:\(sheet.sheetId)"
        }
    }
}

extension BridgeTrackMapping {
    /// This mapping's identity in the table.
    var rowId: String {
        switch source {
        case .file(let file): "file:\(file.fileId)"
        case .sheetEntry(let entry): "entry:\(entry.sheetId):\(entry.index)"
        }
    }
}
