import BaeKit
import SwiftUI

/// A folder that looked like a release but failed validation. Shows a warning
/// icon and the reason; it has no Skip action and no detail pane (selecting it
/// is a no-op — its key isn't a real candidate).
struct InvalidCandidateRow: View {
    let displayName: String
    let reason: BridgeInvalidReason
    /// Real filesystem path for Reveal in Finder.
    let revealPath: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(.orange)
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(displayName)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(reason.localizedText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .help(reason.localizedText)
        .contextMenu {
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: revealPath)
            }
        }
    }
}
