import BaeKit
import SwiftUI

/// A folder that failed validation, looked into the way a Found release is,
/// and read-only: the folder it is, a card with its own cover and what its
/// tags call the release — and, where a release's Import would be, why it
/// can't be imported — then its images and every other file it holds, each
/// damaged one marked with why.
struct InvalidCandidatePane: View {
    let detail: BridgeInvalidCandidateDetail
    /// The exact source window currently auditioning, if any.
    let previewingTarget: BridgePreviewTarget?
    let mappingActions: ImportMappingActions
    let onNavigateToPlacement: () -> Void

    private var invalid: BridgeInvalidCandidate { detail.candidate }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacImportFolderLine(
                    tab: .skipped,
                    folderName: invalid.sourceFolderName,
                    folderPaths: [invalid.folderPath],
                    onNavigateToPlacement: onNavigateToPlacement
                )
                MacInvalidCandidateCard(invalid: invalid)
                if !detail.images.isEmpty {
                    MacImportImages(
                        images: detail.images,
                        onOpen: mappingActions.openImages
                    )
                }
                if !detail.files.isEmpty {
                    files
                }
            }
            .padding(.horizontal, ThemeSpace.section)
            .padding(.top, ThemeSpace.section)
            .padding(.bottom, ThemeSpace.page)
        }
    }

    /// Every file that is not an image, under the heading every release's
    /// files have.
    private var files: some View {
        MacReleaseFiles(
            rows: detail.files.map { .file(file: $0) },
            onOpenDocument: mappingActions.openDocument,
            playing: (previewingTarget, mappingActions)
        )
    }
}
