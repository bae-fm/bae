import BaeKit
import SwiftUI

/// A skipped folder's card: why the folder can't be imported, over its cover
/// and what its tags call the release, or the folder's name when they call
/// it nothing.
struct MacInvalidCandidateCard: View {
    let invalid: BridgeInvalidCandidate

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            MacNotice(
                title: invalid.reason.localizedText,
                advice: nil,
                error: nil,
                tone: .warning
            )
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                ImportCoverWell(
                    coverContent: invalid.cover.map(ImageContent.init(bridge:)),
                    hasCoverOptions: false,
                    onEditCover: {},
                    onDropImage: nil
                )
                if let tags = invalid.tags {
                    MacImportReleaseSummary(
                        summary: ImportReleaseSummary(tags: tags),
                        style: .card
                    )
                }
                else {
                    Text(invalid.sourceFolderName)
                        .themeText(.mono)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(.vertical, ThemeSpace.edge)
        .padding(.horizontal, ThemeSpace.section)
        .card()
    }
}
