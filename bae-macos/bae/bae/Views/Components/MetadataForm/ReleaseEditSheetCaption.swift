import BaeKit
import SwiftUI

/// The caption over the tracks one CUE sheet carves from one file: the
/// import pane's sheet caption, naming the disc the import read the sheet
/// as and the file it carves as text.
struct ReleaseEditSheetCaption: View {
    let sheet: BridgeReleaseEditSheet
    let source: BridgeReleaseEditTrackSource
    let onOpenDocument: (String, String) -> Void

    var body: some View {
        MacImportSheetCaption(
            name: sheet.name,
            onOpen: { onOpenDocument(sheet.name, sheet.fileId) },
            disc: {
                Text(
                    verbatim: localizedCoreString(
                        "ui.import.sheet.disc",
                        Int(sheet.discNumber)
                    )
                )
                .themeText(.strong)
                .fixedSize()
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
