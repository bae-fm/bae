import BaeKit
import SwiftUI

/// Section 2 of the mapping pane: every source the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet describes how playable rows are
/// carved, so its caption sits over the exact rows it owns, outside the
/// columns.
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
    let editingCommands: EditingCommitCommands

    /// The width the pane leaves the table. The columns are resolved against
    /// it, and the table is laid out at it or at its own minimum, whichever is
    /// wider — so the pane never has more table than it has room for, and the
    /// row never has less than its columns need.
    @State
    private var paneWidth: CGFloat = ReleaseMetadataTrackColumns
        .minimumTableWidth
    @State
    var artistFillSelection: ArtistFillSelection?
    @State
    var artistCellFrames: [String: CGRect] = [:]
    /// The track row under the pointer — where the artist fill handle shows.
    @State
    var hoveredFillTrackId: String?

    let artistFillCoordinateSpace = "ImportMappingTable.artistFill"

    private var tableWidth: CGFloat {
        max(paneWidth, ReleaseMetadataTrackColumns.minimumTableWidth)
    }

    private var columns: ReleaseMetadataTrackColumns {
        ReleaseMetadataTrackColumns.resolved(tableWidth: tableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
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

    /// One run of rows. Each section is named by its own leading column
    /// header, not a heading above the table. A pane too narrow for the
    /// columns scrolls sideways rather than squeezing a column past the point
    /// it says anything, and both sections scroll as one so their columns stay
    /// aligned.
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

    /// Core supplies one section per side or disc. Each section contains either
    /// independent track mappings or one sheet and its entries; this view only
    /// renders that shape.
    private var tracksSection: some View {
        ScrollView(.horizontal) {
            artistFillRows {
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
        Text(verbatim: text)
            .font(.system(size: 10, weight: .bold))
            .tracking(1.2)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, index == 0 ? 2 : 18)
            .padding(.bottom, 6)
    }

    private func sheetCaption(_ sheet: BridgeSheetGroup) -> some View {
        sheetCaptionRow(sheet)
            .padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.top, 2)
            .padding(.bottom, 10)
    }

    private func sheetCaptionRow(_ sheet: BridgeSheetGroup) -> some View {
        ImportSheetCaptionRow(
            sheet: sheet,
            options: bindingOptions[sheet.sheetId],
            evidence: ImportEvidence.of(sheet.sheetId, in: evidence),
            showsDiscMenu: table.sheetCount > 1 || sheet.assignment == .ignored,
            actions: actions,
        )
    }

    private func trackRow(_ mapping: BridgeTrackMapping) -> some View {
        ImportMappingTrackRow(
            mapping: mapping,
            columns: columns,
            audioChoices: table.audioChoices,
            previewingTarget: previewingTarget,
            editingCommands: editingCommands,
            evidence: evidenceFor(mapping),
            actions: actions,
            artistFillCoordinateSpace: artistFillCoordinateSpace,
            onArtistFillHover: { hovering in
                guard let trackId = mapping.track?.id else { return }
                if hovering {
                    hoveredFillTrackId = trackId
                }
                else if hoveredFillTrackId == trackId {
                    hoveredFillTrackId = nil
                }
            },
        )
        .rowChrome(
            background: mapping.source.previewTarget == previewingTarget
                ? Theme.accentSoft : .clear
        )
    }

    // Each section's leading header cell carries the section's name — the
    // rows under it are filenames, so the cell names the section rather than
    // the column. The tracks cell also states how the folder and the release
    // disagree about the count, when they do.
    private var trackHeaderRow: some View {
        headerRow {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                FormEyebrow(text: Text("Source"))
                if let reconciliation = table.reconciliation
                    .flatMap(bridgeSlotReconciliationText)
                {
                    Text(reconciliation)
                        .font(.system(size: 11))
                        .monospacedDigit()
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .frame(width: columns.source, alignment: .leading)
            FormEyebrow(text: Text("Track"))
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            // Inset to the fields' text, which sits an inline chrome-pad
            // inside each column.
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
            Color.clear
                .frame(width: ImportMappingColumns.action)
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
        FormEyebrow(text: Text(verbatim: coreString(key)))
    }

    private func headerRow<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            content()
        }
        .padding(.horizontal, ImportMappingColumns.rowPadding)
        .padding(.top, 4)
        .padding(.bottom, 6)
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
    /// height, and a hairline over it. No box and no striping — the table sits
    /// open on the pane under its ruled heading, the columns are what a reader
    /// follows across a row, and a tinted band under half of them is a
    /// second, competing grouping.
    fileprivate func rowChrome(background: Color = .clear) -> some View {
        padding(.horizontal, ImportMappingColumns.rowPadding)
            .padding(.vertical, 6)
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
    static let action = ReleaseMetadataTrackColumns.action
    static let spacing = ReleaseMetadataTrackColumns.spacing
    static let rowPadding = ReleaseMetadataTrackColumns.rowPadding
}

extension BridgeMappingTable {
    /// How many track sheets the folder holds, carving rows or not.
    var sheetCount: Int {
        trackSections.filter {
            if case .sheet = $0.content { return true }
            return false
        }
        .count
            + files.filter {
                if case .sheet = $0 { return true }
                return false
            }
            .count
    }
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
        case .missing: "track:\(track?.id ?? "")"
        }
    }
}
