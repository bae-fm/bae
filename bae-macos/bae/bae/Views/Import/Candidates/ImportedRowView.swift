import BaeKit
import SwiftUI

/// A Done folder and every library copy carrying its content hash, with the
/// hint that the library probably has it when none was imported from it.
struct ImportedRowView: View {
    let row: BridgeImportedRow
    let uploadObservations: [String: UploadObservation]
    let onReveal: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            if row.probablyInLibrary {
                Text(verbatim: coreString("ui.import.probably_in_library"))
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .padding(
                        .horizontal,
                        ImportListHierarchyLayout.rowEdgePadding
                    )
            }
            ForEach(row.releases, id: \.releaseId) { release in
                ImportedReleaseRowView(
                    release: release,
                    candidateKey: row.candidateKey,
                    importStanding: row.live.import,
                    uploadObservation: uploadObservations[release.releaseId]
                )
            }
        }
        .contextMenu {
            Button("Reveal in Finder", action: onReveal)
        }
    }
}

private struct ImportedReleaseRowView: View {
    let release: BridgeImportedReleaseSummary
    let candidateKey: String
    let importStanding: BridgeImportStanding?

    let uploadObservation: UploadObservation?

    var body: some View {
        HStack(alignment: .center, spacing: ThemeSpace.related) {
            ImageView(
                imageRef: release.cover,
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
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            ImportReleaseSummaryView(
                summary: ImportReleaseSummary(release: release),
                style: .sidebar
            ) {
                RecordArrow(readFromRecord: release.readFromRecord)
            }
            switch importStanding {
            case .running, .writing:
                // Subscribes to progress here because it changes every second.
                ImportProgressLine(key: candidateKey)
                    .themeText(.detail)
            case .queued:
                EmptyView()
            case nil:
                if let uploadObservation {
                    ProgressLine(
                        uploadObservation.phaseText,
                        progress: uploadObservation.progressBar.fraction
                    )
                    .themeText(.detail)
                }
            }
        }
    }

    /// A queued import's clock, or the storage queue's upload arrow while the
    /// release is still uploading.
    @ViewBuilder
    private var trailing: some View {
        switch importStanding {
        case .queued:
            QueuedImportIcon()
                .fixedSize()
        case .running, .writing:
            EmptyView()
        case nil:
            if case .active = uploadObservation {
                Image(systemName: "arrow.up.circle")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
                    .fixedSize()
            }
        }
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Done rows") {
        VStack(alignment: .leading, spacing: 0) {
            ImportedRowView(
                row: PreviewData.importedRowIdentified,
                uploadObservations: [:],
                onReveal: {}
            )
            ImportedRowView(
                row: PreviewData.importedRowFromTags,
                uploadObservations: [:],
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
