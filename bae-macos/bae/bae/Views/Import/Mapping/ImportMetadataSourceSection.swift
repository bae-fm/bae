import BaeKit
import SwiftUI

/// The pressings identification matched, offered inline while nothing is
/// picked — the question the section is asking, so the answer choices render
/// right under it rather than behind the search sheet.
struct ImportMatchOptions {
    /// One card per release group the matches fall into, in match order.
    let groups: [ReleaseGroup]
    let libraryStatuses: [String: BridgeLibraryStatus]
    let provenance: [String: BridgeResultProvenance]
    let isImporting: Bool
    /// The pressing whose pick is being read right now, carrying the row
    /// spinner. `nil` when nothing is in flight.
    let loadingReleaseId: String?
    let onSelect: (BridgeMetadataResult) -> Void
}

/// Section 1 of the mapping pane: the metadata source being inspected and the
/// explicit action that chooses it for import.
struct ImportMetadataSourceSection: View {
    let mode: BridgeImportMetadataMode
    let releaseSummary: ImportReleaseSummary?
    let fileTagsPreviewSummary: ImportReleaseSummary?
    let isReading: Bool
    let coverContent: ImageContent?
    let hasCoverOptions: Bool
    let editValues: BridgeRawReleaseEdit?
    let editActions: ReleaseFieldWriter
    let matchOptions: ImportMatchOptions?
    let hasSelectedSeed: Bool
    let commit: ImportCommitControls?
    let onPresentMode: (BridgeImportMetadataMode) -> Void
    let onFindRelease: () -> Void
    let onReadFileTags: () -> Void
    let onUseFileTags: () -> Void
    let onEnterManually: () -> Void
    let onEditCover: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            FormSectionHeader(title: coreString("ui.import.metadata.title"))
            VStack(alignment: .leading, spacing: 10) {
                modePicker
                modeContent
            }
        }
    }

    @ViewBuilder
    private var modeContent: some View {
        switch mode {
        case .lookup:
            lookupContent
        case .fileTags:
            fileTagsContent
        case .manual:
            manualContent
        }
    }

    @ViewBuilder
    private var lookupContent: some View {
        if let matchOptions {
            ForEach(matchOptions.groups) { group in
                ReleaseGroupSection(
                    group: group,
                    isImporting: matchOptions.isImporting,
                    libraryStatuses: matchOptions.libraryStatuses,
                    provenance: matchOptions.provenance,
                    selectedReleaseId: nil,
                    loadingReleaseId: matchOptions.loadingReleaseId,
                    onSelect: matchOptions.onSelect,
                )
            }
            Button(coreString("ui.import.header.find_release")) {
                onFindRelease()
            }
            .buttonStyle(.bordered)
            .disabled(isReading)
        }
        else if hasSelectedSeed {
            releaseHeader(
                summary: selectedReleaseSummary,
                action: .changeRelease,
                onAction: onFindRelease,
                includesSelectedValues: true
            )
        }
        else {
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                    .opacity(isReading ? 1 : 0)
                Button(coreString("ui.import.header.find_release")) {
                    onFindRelease()
                }
                .buttonStyle(.borderedProminent)
            }
            .disabled(isReading)
        }
    }

    @ViewBuilder
    private var fileTagsContent: some View {
        if hasSelectedSeed {
            releaseHeader(
                summary: selectedReleaseSummary,
                action: nil,
                onAction: {},
                includesSelectedValues: true
            )
        }
        else if let fileTagsPreviewSummary {
            releaseHeader(
                summary: fileTagsPreviewSummary,
                action: .useFileTags,
                onAction: onUseFileTags,
                includesSelectedValues: false
            )
        }
        else {
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                    .opacity(isReading ? 1 : 0)
                Button(coreString("ui.import.metadata.file_tags")) {
                    onReadFileTags()
                }
                .buttonStyle(.borderedProminent)
                .opacity(isReading ? 0 : 1)
                .allowsHitTesting(!isReading)
            }
        }
    }

    private var manualContent: some View {
        releaseHeader(
            summary: hasSelectedSeed ? selectedReleaseSummary : .manual,
            action: hasSelectedSeed ? nil : .enterManually,
            onAction: onEnterManually,
            includesSelectedValues: hasSelectedSeed
        )
    }

    private var selectedReleaseSummary: ImportReleaseSummary {
        guard let releaseSummary else {
            preconditionFailure(
                "a selected metadata seed must carry its release summary"
            )
        }
        return releaseSummary
    }

    private func releaseHeader(
        summary: ImportReleaseSummary,
        action: ImportReleaseHeaderAction?,
        onAction: @escaping () -> Void,
        includesSelectedValues: Bool
    ) -> some View {
        ImportReleaseHeader(
            releaseSummary: summary,
            action: action,
            isReading: isReading,
            coverContent: includesSelectedValues ? coverContent : nil,
            hasCoverOptions: includesSelectedValues && hasCoverOptions,
            editValues: includesSelectedValues ? editValues : nil,
            editActions: editActions,
            commit: includesSelectedValues ? commit : nil,
            onEditCover: onEditCover,
            onAction: onAction
        )
    }

    private var modePicker: some View {
        Picker(
            coreString("ui.import.metadata.title"),
            selection: Binding(
                get: { mode },
                set: { onPresentMode($0) },
            )
        ) {
            Text(coreString("ui.import.metadata.lookup"))
                .tag(BridgeImportMetadataMode.lookup)
            Text(coreString("ui.import.metadata.file_tags"))
                .tag(BridgeImportMetadataMode.fileTags)
            Text(coreString("ui.import.metadata.manual"))
                .tag(BridgeImportMetadataMode.manual)
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .disabled(isReading)
        .fixedSize()
    }
}
