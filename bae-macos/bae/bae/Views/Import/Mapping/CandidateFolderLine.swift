import BaeKit
import SwiftUI

/// The folder the pane is about: the queue tab it is on, which goes there, a
/// glyph that reveals it in Finder, and its selectable name.
struct CandidateFolderLine: View {
    /// The tab the queue places the folder on. `nil` for a folder the queue
    /// does not hold.
    let tab: BridgeTriageTab?
    let folderName: String
    /// The folders the glyph reveals in Finder, as coven's bytes for each
    /// path.
    let folderPaths: [Data]
    let onNavigateToPlacement: () -> Void

    static func label(for tab: BridgeTriageTab) -> String {
        return switch tab {
        case .pending: String(localized: "Found")
        case .done: String(localized: "Imported")
        case .skipped: String(localized: "Skipped")
        }
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            if let tab {
                Button(action: onNavigateToPlacement) {
                    StatusChip(verbatim: Self.label(for: tab), tone: .accent)
                }
                .buttonStyle(.accentPlain)
                .fixedSize()
                Image(systemName: "chevron.forward")
                    .themeIcon(.small)
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
            }
            Button {
                for path in folderPaths {
                    SystemActions.revealInFinder(folder: path)
                }
            } label: {
                Image(systemName: "folder")
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.accentPlain)
            .help("Reveal in Finder")
            .accessibilityLabel("Reveal in Finder")
            Text(folderName)
                .themeText(.heading)
                .textSelection(.enabled)
                .cutLine(.fileName)
            Spacer(minLength: 0)
        }
    }
}
