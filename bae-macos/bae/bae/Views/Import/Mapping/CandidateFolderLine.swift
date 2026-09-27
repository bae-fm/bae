import BaeKit
import SwiftUI

/// The folder the pane is about: the queue tab it is on, which goes there, a
/// glyph that reveals it in Finder, and its selectable name.
struct CandidateFolderLine: View {
    /// The tab the queue places the folder on. `nil` for a folder the queue
    /// does not hold.
    let tab: BridgeTriageTab?
    let folderName: String
    /// The paths the folder glyph reveals in Finder.
    let folderPaths: [String]
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
                    Text(Self.label(for: tab))
                        .themeText(.chip)
                        .lineLimit(1)
                        .padding(.horizontal, ThemeSpace.related)
                        .padding(.vertical, ThemeSpace.line)
                        .background(Theme.accentSoft, in: Capsule())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Theme.accent)
                .fixedSize()
                Image(systemName: "chevron.right")
                    .themeIcon(.small)
                    .foregroundStyle(.tertiary)
            }
            Button {
                for path in folderPaths {
                    SystemActions.revealInFinder(path: path)
                }
            } label: {
                Image(systemName: "folder")
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Reveal in Finder")
            Text(folderName)
                .themeText(.heading)
                .textSelection(.enabled)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
        }
    }
}

#if DEBUG
    #Preview("Candidate folder line") {
        CandidateFolderLine(
            tab: .pending,
            folderName:
                "2010 \u{2013} Blue Sky Boys 1939\u{2013}1940 (256 kbps)",
            folderPaths: ["/Music/Blue Sky Boys"],
            onNavigateToPlacement: {}
        )
        .padding()
        .frame(width: 520)
        .windowBackground()
    }

#endif
