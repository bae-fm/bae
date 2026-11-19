import BaeKit
import SwiftUI

/// The files a release carries that are no track, such as a rip log or a
/// cover image's sibling files: a ruled Files heading, then one line per
/// file, its name in mono and its size at the right, a hairline between
/// lines. A document opens when clicked.
///
/// The one way a release's files show, under its tracks.
struct MacReleaseFiles: View {
    let rows: [BridgeMappingFileRow]
    /// Open a document in the viewer: the file's name, then its file id.
    let onOpenDocument: (String, String) -> Void
    /// Where a Found folder's track sheets can still be bound to audio and
    /// given a disc, the actions that do it; `nil` where the sheets are
    /// read-only.
    var sheetActions: ImportMappingActions?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacFormHeading(title: String(localized: "Files"))
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.rowId) {
                    index,
                    row in
                    line(row)
                        .padding(.horizontal, ThemeSpace.line)
                        .padding(.vertical, ThemeSpace.related)
                        .overlay(alignment: .top) {
                            Rectangle()
                                .fill(Theme.hairline)
                                .frame(height: 1)
                                .opacity(index > 0 ? 1 : 0)
                        }
                }
            }
        }
    }

    @ViewBuilder
    private func line(_ row: BridgeMappingFileRow) -> some View {
        switch row {
        case .file(let file):
            MacReleaseFileRow(
                name: file.name,
                size: file.sizeText,
                damage: file.damageText,
                onOpen: file.role == .document
                    ? { onOpenDocument(file.name, file.fileId) } : nil
            )
        case .sheet(let sheet):
            if let sheetActions {
                MacImportSheetCaption(
                    sheet: sheet,
                    showsDiscMenu: true,
                    actions: sheetActions
                )
            }
            else {
                MacReleaseFileRow(
                    name: sheet.name,
                    size: sheet.size.fileSizeText,
                    onOpen: { onOpenDocument(sheet.name, sheet.sheetId) }
                )
            }
        }
    }
}
