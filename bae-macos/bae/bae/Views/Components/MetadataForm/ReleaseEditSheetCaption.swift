import BaeKit
import SwiftUI

/// The caption over the tracks one CUE sheet carves from one file: the
/// import pane's sheet caption, with the Disc pop-up of which of the
/// release's discs the sheet is, the sheet, and the file it carves as text.
struct ReleaseEditSheetCaption: View {
    let sheet: BridgeReleaseEditSheet
    let source: BridgeReleaseEditTrackSource
    let onOpenDocument: (String, String) -> Void
    /// The person picked a disc for the sheet.
    let onPickDisc: (UInt32) -> Void

    var body: some View {
        MacImportSheetCaption(
            name: sheet.name,
            onOpen: { onOpenDocument(sheet.name, sheet.fileId) },
            disc: {
                SheetDiscMenu(
                    discs: sheet.discOptions,
                    assignment: .disc(number: sheet.discNumber),
                    offersIgnored: false,
                    onAssign: { assignment in
                        if case .disc(let number) = assignment {
                            onPickDisc(number)
                        }
                    }
                )
            },
            audio: {
                Text(verbatim: source.name)
                    .themeText(.mono)
                    .foregroundStyle(.secondary)
                    .cutLine(.fileName)
                    .help(source.name)
            }
        )
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, ThemeSpace.related)
        .padding(.bottom, ThemeSpace.compact)
    }
}
