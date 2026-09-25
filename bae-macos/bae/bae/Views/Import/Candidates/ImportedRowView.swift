import BaeKit
import SwiftUI

/// One Done row: the library release the candidate became — its cover, title,
/// artist and year, and whether a catalog describes it — as the library has
/// it now. Core reads all of it from the library inside the list's own query,
/// so re-identifying, editing or re-covering the release reaches this row with
/// no candidate state in between.
///
/// An import that just wrote the release can own the candidate for a moment
/// longer; that comes from the row's own live-state subscription, as it does
/// for every other row.
struct ImportedRowView: View {
    let row: BridgeImportedRow
    /// The release's cloud transition, resolved by the list owner that
    /// already observes the outbox.
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
        HStack(alignment: .center, spacing: 10) {
            ImageView(
                imageRef: row.release.cover,
                pointSize: TriageRowView.coverPointSize
            )
            .frame(
                width: TriageRowView.coverPointSize,
                height: TriageRowView.coverPointSize
            )
            .clipShape(RoundedRectangle(cornerRadius: 6))
            meta
            Spacer(minLength: 4)
            trailing
        }
        .padding(.vertical, 6)
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
                // The one line that changes by the second, so it subscribes to
                // the candidate-runtime signal at this leaf.
                ImportProgressLine(key: row.candidateKey)
                    .font(.system(size: 11.5))
            }
            else if let uploadObservation {
                ProgressLine(
                    uploadObservation.phaseText,
                    progress: uploadObservation.progressBar.fraction
                )
                .font(.system(size: 11.5))
            }
        }
    }

    /// Still going up to the cloud — the same arrow the storage queue marks
    /// an active upload with, and nothing else: the release is in the library
    /// either way.
    @ViewBuilder
    private var trailing: some View {
        if !importing, case .active = uploadObservation {
            Image(systemName: "arrow.up.circle")
                .font(.caption)
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
