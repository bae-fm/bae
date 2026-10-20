import BaeKit
import SwiftUI

/// A folder that looked like a release but failed validation, drawn the way a
/// Found row is: its own cover, what its tags call the release — or its
/// folder's name when they name none — and, under it, why it can't be
/// imported. Selecting it opens the pane that looks into it; it offers nothing
/// to do.
struct InvalidCandidateRow: View {
    let invalid: BridgeInvalidCandidate
    let isGroupMember: Bool
    /// Read the folders a grouping took in as the releases they are again.
    let onSeparate: () -> Void

    var body: some View {
        ImportListRowLayout(
            coverContent: invalid.cover.map(ImageContent.init(bridge:))
        ) {
            VStack(alignment: .leading, spacing: 0) {
                if let tags = invalid.tags {
                    ImportReleaseSummaryView(
                        summary: ImportReleaseSummary(tags: tags),
                        style: .sidebar
                    )
                }
                else {
                    ImportFolderNameLine(folderName: invalid.sourceFolderName)
                }
                Text(invalid.reason.localizedText)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .groupMemberRail(isGroupMember)
        .contentShape(Rectangle())
        .help(invalid.reason.localizedText)
        .contextMenu {
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: invalid.folderPath)
            }
            // An unreadable combined folder can only be separated from here.
            if invalid.separable {
                Button("Keep as Separate Releases", action: onSeparate)
            }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Skipped rows") {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(PreviewData.invalidCandidates, id: \.candidateKey) {
                invalid in
                InvalidCandidateRow(
                    invalid: invalid,
                    isGroupMember: false,
                    onSeparate: {}
                )
            }
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .windowBackground()
    }
#endif
