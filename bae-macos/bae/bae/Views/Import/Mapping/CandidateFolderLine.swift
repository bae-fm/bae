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
        HStack(spacing: 8) {
            if let tab {
                Button(action: onNavigateToPlacement) {
                    Text(Self.label(for: tab))
                        .font(.caption.weight(.medium))
                        .lineLimit(1)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(Theme.accentSoft, in: Capsule())
                }
                .buttonStyle(.plain)
                .foregroundStyle(Theme.accent)
                .fixedSize()
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            Button {
                for path in folderPaths {
                    SystemActions.revealInFinder(path: path)
                }
            } label: {
                Image(systemName: "folder")
                    .font(.system(size: 14))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Reveal in Finder")
            Text(folderName)
                .font(.system(size: 15, design: .monospaced))
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
