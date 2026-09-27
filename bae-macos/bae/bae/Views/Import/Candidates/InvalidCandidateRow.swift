import BaeKit
import SwiftUI

/// A folder that looked like a release but failed validation, shown with the
/// reason. It is not a real candidate, so it has no Skip action or detail pane.
struct InvalidCandidateRow: View {
    let displayName: String
    let reason: BridgeInvalidReason
    /// Real filesystem path for Reveal in Finder.
    let revealPath: String

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "exclamationmark.triangle")
                .themeIcon(.medium)
                .foregroundStyle(Theme.warning)
                .frame(width: ThemeIcon.medium.size)
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(displayName)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(reason.localizedText)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
        }
        .padding(.vertical, ThemeSpace.inline)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
        .contentShape(Rectangle())
        .help(reason.localizedText)
        .contextMenu {
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: revealPath)
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Invalid candidate") {
        InvalidCandidateRow(
            displayName: "Broken Rip",
            reason: .corruptAudioFile(path: "03.flac"),
            revealPath: "/Music/Downloads/Broken Rip",
        )
        .padding()
        .frame(width: 320)
        .windowBackground()
    }
#endif
