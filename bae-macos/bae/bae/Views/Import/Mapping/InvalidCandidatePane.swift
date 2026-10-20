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
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                CandidateFolderLine(
                    tab: .skipped,
                    folderName: invalid.sourceFolderName,
                    folderPaths: [invalid.folderPath],
                    onNavigateToPlacement: onNavigateToPlacement
                )
                card
                if !detail.images.isEmpty {
                    VStack(alignment: .leading, spacing: ThemeSpace.related) {
                        FormSectionHeader(
                            title: String(localized: "Images"),
                            ruled: true
                        )
                        ImportMappingGallery(
                            images: detail.images,
                            onOpen: mappingActions.openImages
                        )
                    }
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

    /// The release card: why the folder can't be imported where Import would
    /// be, over its cover and what its tags call it.
    private var card: some View {
        VStack(alignment: .leading, spacing: ReleaseMetadataLayout.blockSpacing)
        {
            HStack(alignment: .center, spacing: ThemeSpace.edge) {
                Spacer(minLength: ThemeSpace.group)
                Label {
                    Text(invalid.reason.localizedText)
                        .themeText(.strong)
                } icon: {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .themeIcon(.small)
                }
                .foregroundStyle(Theme.warning)
                .help(invalid.reason.localizedText)
            }
            HStack(alignment: .top, spacing: ThemeSpace.group) {
                ImportCoverWell(
                    coverContent: invalid.cover.map(ImageContent.init(bridge:)),
                    hasCoverOptions: false,
                    localCoverSelections: [:],
                    onEditCover: {},
                    onSelectCover: { _ in }
                )
                if let tags = invalid.tags {
                    ImportReleaseSummaryView(
                        summary: ImportReleaseSummary(tags: tags),
                        style: .card
                    )
                }
                else {
                    ImportFolderNameLine(folderName: invalid.sourceFolderName)
                }
            }
        }
        .padding(.vertical, ThemeSpace.edge)
        .padding(.horizontal, ThemeSpace.section)
        .card()
    }

    /// Every file that is not an image, under the heading the mapping table
    /// gives its own files.
    private var files: some View {
        VStack(spacing: 0) {
            Eyebrow(verbatim: coreString("ui.import.mapping.files_title"))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, ImportMappingColumns.rowPadding)
                .padding(.top, ThemeSpace.inline)
                .padding(.bottom, ThemeSpace.compact)
            ForEach(detail.files, id: \.fileId) { file in
                ImportMappingFileRow(
                    file: file,
                    previewingTarget: previewingTarget,
                    evidence: [],
                    actions: mappingActions
                )
                .mappingRowChrome(
                    background: file.previewTarget != nil
                        && file.previewTarget == previewingTarget
                        ? Theme.accentSoft : .clear
                )
            }
        }
    }
}

#if DEBUG
    #Preview("Skipped folder pane") {
        InvalidCandidatePane(
            detail: PreviewData.invalidCandidateDetail,
            previewingTarget: nil,
            mappingActions: inertMappingActions(),
            onNavigateToPlacement: {}
        )
        .frame(width: 900, height: 700)
        .importPreviewEnvironment()
        .environment(PreviewData.artImageStore())
    }

    #Preview("Skipped folder pane — no tags, no images") {
        InvalidCandidatePane(
            detail: PreviewData.untaggedInvalidCandidateDetail,
            previewingTarget: nil,
            mappingActions: inertMappingActions(),
            onNavigateToPlacement: {}
        )
        .frame(width: 900, height: 500)
        .importPreviewEnvironment()
        .environment(PreviewData.artImageStore())
    }
#endif
