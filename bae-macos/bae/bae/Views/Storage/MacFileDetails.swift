import BaeKit
import SwiftUI

/// The details of the file selected in the storage inspector: its name, the
/// folder it sits in, what kind of file it is, its audio format and its size,
/// with the button that closes them.
struct MacFileDetails: View {
    let file: BridgeFile
    let onClose: () -> Void

    /// The inset above the details.
    private static let topInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Text(file.name)
                    .themeText(.heading)
                    .cutLine(.fileName)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button(action: onClose) {
                    Image(systemName: "xmark.circle")
                        .themeIcon(.medium)
                        .foregroundStyle(Theme.macTextSecondary)
                }
                .buttonStyle(.accentPlain)
                .accessibilityLabel(Text("Close file details"))
                .help(Text("Close file details"))
            }
            MacFactsGrid {
                if let folder = file.folder {
                    MacFact("Folder") { Text(folder) }
                }
                MacFact("Kind") { Text(kindText) }
                if let format = file.audioFormat {
                    MacFact("Format") { Text(format.text) }
                }
                MacFact("Size") {
                    Text(file.fileSizeText).monospacedDigit()
                }
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, Self.topInset)
        .padding(.bottom, ThemeSpace.edge)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surfaceElevated)
    }

    private var kindText: String {
        switch file.kind {
        case .audio(let format): String(localized: "\(format) audio")
        case .image(let format): String(localized: "\(format) image")
        case .damagedImage: String(localized: "Damaged image")
        case .cueSheet: String(localized: "Cue sheet")
        case .ripLog: String(localized: "Rip log")
        case .accurateRipReport: String(localized: "AccurateRip report")
        case .playlist: String(localized: "Playlist")
        case .text: String(localized: "Text")
        case .pdf: String(localized: "PDF document")
        case .other: String(localized: "File")
        }
    }
}
