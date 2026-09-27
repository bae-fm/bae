import BaeKit
import SwiftUI

/// One Done row: the library release the candidate became, as the library
/// has it now.
struct ImportedRowView: View {
    let row: BridgeImportedRow
    /// The release's cloud upload state, from the list owner.
    let uploadObservation: UploadObservation?
    let onReveal: () -> Void

    var body: some View {
        CandidateLiveStateReader(
            key: row.candidateKey,
            basis: row.actionBasis
        ) { live in
            ImportedRowContent(
                row: row,
                importing: live?.importing == true,
                uploadObservation: uploadObservation,
                onReveal: onReveal
            )
        }
    }
}

/// A Done row drawn from its release and what is running for its candidate.
struct ImportedRowContent: View {
    let row: BridgeImportedRow
    let importing: Bool
    let uploadObservation: UploadObservation?
    let onReveal: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: ThemeSpace.related) {
            ImageView(
                imageRef: row.release.cover,
                pointSize: TriageRowView.coverPointSize
            )
            .frame(
                width: TriageRowView.coverPointSize,
                height: TriageRowView.coverPointSize
            )
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            meta
            Spacer(minLength: ThemeSpace.inline)
            trailing
        }
        .padding(.vertical, ThemeSpace.compact)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
        .contentShape(Rectangle())
        .contextMenu {
            Button("Reveal in Finder", action: onReveal)
        }
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            ImportReleaseSummaryView(
                summary: ImportReleaseSummary(release: row.release),
                style: .sidebar
            ) {
                RecordArrow(readFromRecord: row.release.readFromRecord)
            }
            if importing {
                // Subscribes to progress here because it changes every second.
                ImportProgressLine(key: row.candidateKey)
                    .themeText(.detail)
            }
            else if let uploadObservation {
                ProgressLine(
                    uploadObservation.phaseText,
                    progress: uploadObservation.progressBar.fraction
                )
                .themeText(.detail)
            }
        }
    }

    /// The storage queue's upload arrow while the release is still uploading.
    @ViewBuilder
    private var trailing: some View {
        if !importing, case .active = uploadObservation {
            Image(systemName: "arrow.up.circle")
                .themeIcon(.small)
                .foregroundStyle(.secondary)
                .fixedSize()
        }
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Done rows") {
        VStack(alignment: .leading, spacing: 0) {
            ImportedRowView(
                row: PreviewData.importedRowIdentified,
                uploadObservation: nil,
                onReveal: {}
            )
            ImportedRowView(
                row: PreviewData.importedRowFromTags,
                uploadObservation: nil,
                onReveal: {}
            )
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
#endif
