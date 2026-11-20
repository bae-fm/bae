import BaeKit
import SwiftUI

/// The caption over the tracks one CUE sheet carves from one file: the
/// import pane's sheet caption, with the Disc pop-up of which of the
/// release's discs the sheet is, the sheet, and the Audio file pop-up of
/// which of the release's audio files on this device the sheet's FILE line
/// plays from.
struct ReleaseEditSheetCaption: View {
    let sheet: BridgeReleaseEditSheet
    let source: BridgeReleaseEditTrackSource
    let onOpenDocument: (String, String) -> Void
    /// The person picked a disc for the sheet.
    let onPickDisc: (UInt32) -> Void
    /// The person picked an audio file for the sheet's FILE line, by the
    /// file's id.
    let onPickFile: (String) -> Void

    @State
    private var hoveringAudio = false

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
            audio: { audioMenu }
        )
        .padding(.horizontal, ReleaseMetadataTrackColumns.rowPadding)
        .padding(.top, ThemeSpace.related)
        .padding(.bottom, ThemeSpace.compact)
    }

    /// The file the line plays from, as the menu of the files it can play
    /// from: each one core refuses is listed, dimmed, with core's reason.
    private var audioMenu: some View {
        MacButton(
            kind: .plain,
            menu: {
                for option in sheet.audioOptions {
                    if let reason = option.offer.refusalLine {
                        MacMenuItem(
                            verbatim: "\(option.name): \(reason)",
                            disabled: true
                        ) {}
                    }
                    else {
                        MacMenuItem(
                            verbatim: option.name,
                            checked: option.bound
                        ) { onPickFile(option.fileId) }
                    }
                }
            }
        ) {
            Text(verbatim: source.name)
                .themeText(.mono)
                .foregroundStyle(.secondary)
                .cutLine(.fileName)
                .help(source.name)
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .background(
                    hoveringAudio ? Theme.hover : Color.clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .frame(minWidth: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .onHover { hoveringAudio = $0 }
        .accessibilityLabel(String(localized: "Audio file"))
        .accessibilityValue(Text(verbatim: source.name))
    }
}
