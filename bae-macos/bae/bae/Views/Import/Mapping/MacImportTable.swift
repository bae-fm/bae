import BaeKit
import SwiftUI

/// The release editor's track table, in the import pane and in the Edit
/// release sheet alike: a heading for each side or disc, then its runs,
/// each a track row or a CUE sheet's caption over the rows it carves.
struct MacImportTable: View {
    let sides: [BridgeTrackTableSide]
    /// The source window the preview player plays now, if any; its row is
    /// accented.
    let previewingTarget: BridgePreviewTarget?
    let actions: ReleaseTrackTableActions
    let editingCommands: EditingCommitCommands

    @Environment(Library.self)
    private var library
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
        ScrollView(.horizontal) {
            VStack(spacing: 0) {
                trackHeaderRow
                ForEach(Array(sides.enumerated()), id: \.offset) {
                    index,
                    side in
                    if !side.side.headingText.isEmpty {
                        sideHeader(side.side.headingText, index: index)
                    }
                    ForEach(Array(side.runs.enumerated()), id: \.offset) {
                        _,
                        run in
                        if case .sheet(let sheet, _) = run {
                            sheetCaption(sheet)
                        }
                        ForEach(run.rows, id: \.track.id, content: trackRow)
                    }
                }
            }
            .frame(width: tableWidth, alignment: .leading)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .onGeometryChange(for: CGFloat.self) { geo in
            geo.size.width
        } action: {
            paneWidth = $0
        }
    }

    private func sideHeader(_ text: String, index: Int) -> some View {
        MacEyebrow(verbatim: text)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, index == 0 ? ThemeSpace.line : ThemeSpace.edge)
            .padding(.bottom, ThemeSpace.compact)
    }

    private func sheetCaption(_ sheet: BridgeSheetGroup) -> some View {
        ReleaseSheetCaption(sheet: sheet, actions: actions.sheetCaption)
    }

    private func trackRow(_ row: BridgeTrackTableRow) -> some View {
        MacImportTrackRow(
            row: row,
            columns: columns,
            previewingTarget: previewingTarget,
            editingCommands: editingCommands,
            actions: actions,
            artistSearch: library.artistSearch,
        )
    }

    // The leading cell names the section.
    private var trackHeaderRow: some View {
        HStack(spacing: ImportMappingColumns.spacing) {
            MacEyebrow("Source")
                .frame(width: columns.source, alignment: .leading)
            MacEyebrow("Track")
                .frame(
                    width: ReleaseMetadataTrackColumns.track,
                    alignment: .leading
                )
            eyebrow("ui.import.mapping.column.title")
                .frame(width: columns.title, alignment: .leading)
            eyebrow("ui.import.mapping.column.artist")
                .frame(width: columns.artist, alignment: .leading)
            eyebrow("ui.import.slots.column.length")
                .frame(
                    width: ReleaseMetadataTrackColumns.length,
                    alignment: .leading
                )
        }
        .padding(.bottom, ThemeSpace.compact)
    }

    private func eyebrow(_ key: String) -> some View {
        MacEyebrow(verbatim: localizedCoreString(key))
    }
}

/// What the track table's controls do, as its host runs them.
struct ReleaseTrackTableActions {
    let sheetCaption: SheetCaptionActions
    let audition: TrackAuditionActions
    /// Write a row's edited title and artists onto the row that edits the
    /// track.
    let editTrack: @MainActor (BridgeRawTrackEdit) async -> Void
}

/// What a row's Play button does, as its host runs it: a folder's row plays
/// a window of its file through the preview player, and a library release's
/// row plays the release from its track.
struct TrackAuditionActions {
    /// Play the exact source window a folder's row carries.
    let preview: (BridgePreviewTarget) -> Void
    /// Stop whatever the preview player plays.
    let stopPreview: () -> Void
    /// Play the library release from the track this id names; `nil` where
    /// the host plays no library track.
    let playTrack: ((String) -> Void)?
}

enum ImportMappingColumns {
    static let spacing = ReleaseMetadataTrackColumns.spacing
}

extension BridgeMappingFileRow {
    var rowId: String {
        switch self {
        case .file(let file): "file:\(file.fileId)"
        case .sheet(let sheet): "sheet:\(sheet.sheetId)"
        }
    }
}
