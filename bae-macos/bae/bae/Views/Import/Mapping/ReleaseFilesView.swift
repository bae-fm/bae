import BaeKit
import SwiftUI

/// The files a release carries that are no track, such as a rip log or a
/// cover image's sibling files: a ruled Files heading, then one line per
/// file, its name in mono and its size at the right, a hairline between
/// lines. A document opens when clicked.
///
/// The one way a release's files show, under its tracks.
struct ReleaseFilesView: View {
    let rows: [BridgeMappingFileRow]
    /// Open a document in the viewer: the file's name, then its file id.
    let onOpenDocument: (String, String) -> Void
    /// Where a Found folder's track sheets can still be bound to audio and
    /// given a disc, the actions that do it; `nil` where the sheets are
    /// read-only.
    var sheetActions: ImportMappingActions?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            FormSectionHeader(title: String(localized: "Files"), ruled: true)
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
            fileLine(file)
        case .sheet(let sheet):
            if let sheetActions {
                ImportSheetCaptionRow(
                    sheet: sheet,
                    showsDiscMenu: true,
                    actions: sheetActions
                )
            }
            else {
                openable(
                    name: sheet.name,
                    size: Int64(sheet.size).formatted(.byteCount(style: .file)),
                    id: sheet.sheetId
                )
            }
        }
    }

    private func fileLine(_ file: BridgeMappingFile) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            if file.role == .document {
                openable(name: file.name, size: file.sizeText, id: file.fileId)
            }
            else {
                content(name: file.name, size: file.sizeText)
            }
            if let damage = file.damageText {
                Text(damage)
                    .themeText(.fine)
                    .foregroundStyle(Theme.warning)
            }
        }
    }

    private func openable(name: String, size: String, id: String) -> some View {
        Button {
            onOpenDocument(name, id)
        } label: {
            content(name: name, size: size)
        }
        .buttonStyle(.plain)
    }

    private func content(name: String, size: String) -> some View {
        HStack(spacing: ThemeSpace.group) {
            Text(name)
                .themeText(.mono)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
            Text(size)
                .themeText(.fine)
                .foregroundStyle(.tertiary)
                // A squeezed row truncates the name, never wraps the size.
                .fixedSize()
        }
        .contentShape(Rectangle())
    }
}
