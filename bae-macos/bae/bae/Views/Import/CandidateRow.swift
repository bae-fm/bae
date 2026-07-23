import BaeKit
import SwiftUI

/// One candidate row in the import list: a status icon, the display name, and a
/// "Likely imported" note for probable duplicates. Skip/reveal live in its
/// context menu.
struct CandidateRow: View {
    let displayName: String
    /// Real filesystem path for Reveal in Finder. Always set for folder
    /// candidates.
    let revealPath: String?
    let status: BridgeCandidateImportStatus?
    /// The candidate was already imported from this folder (content-hash match),
    /// shown the same as an in-session completed import.
    let isAdded: Bool
    /// The user manually skipped this candidate.
    let skipped: Bool
    let isLikelyDupe: Bool
    /// Skip (true) or unskip (false) this candidate.
    let onSkip: (_ skipped: Bool) -> Void

    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        HStack(spacing: 8) {
            statusIcon
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(displayName)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if isLikelyDupe, status == nil, !isAdded, !skipped {
                    Text("Likely imported")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .opacity(isLikelyDupe ? 0.6 : 1.0)
            Spacer()
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .contextMenu {
            Button(skipped ? "Move to New" : "Skip") { onSkip(!skipped) }
            if let revealPath {
                Divider()
                Button("Reveal in Finder") {
                    SystemActions.revealInFinder(path: revealPath)
                }
            }
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if skipped {
            icon("minus.circle", .secondary)
        }
        else if let status {
            switch status {
            case .importing:
                ProgressView()
                    .controlSize(.small)
            case .complete(let releaseId, albumId: _):
                // Imported, but the cloud upload may still be draining — show
                // the transfer until the release's queue empties.
                if outboxStore.progress(forRelease: releaseId) != nil {
                    icon("icloud.and.arrow.up", .secondary)
                }
                else {
                    icon("checkmark.circle.fill", .green)
                }
            case .error:
                icon("exclamationmark.triangle.fill", .red)
            }
        }
        else if isAdded {
            // An already-imported folder (content-hash match) shows the same
            // green check as an in-session completed import.
            icon("checkmark.circle.fill", .green)
        }
        else {
            icon("folder", .secondary)
        }
    }

    private func icon<S: ShapeStyle>(_ systemName: String, _ style: S)
        -> some View
    {
        Image(systemName: systemName).foregroundStyle(style)
    }
}
