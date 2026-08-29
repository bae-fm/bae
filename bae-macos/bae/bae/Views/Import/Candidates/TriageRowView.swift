import BaeKit
import SwiftUI

/// One triage row: an optional bulk-select checkbox, the matched release's
/// cover, its title/metadata, and a trailing tag or status. Every decision
/// about what the row shows — which tab, which group, whether it takes a
/// checkbox — is `row`'s, read off `BridgeTriageRow`; this only renders it.
///
/// A row does not change height on selection: the folder it came from is the
/// main pane's to state, and a row that grows on selection shifts every row
/// under it.
struct TriageRowView: View {
    /// The cover's edge, in points. Named because it is also the size the
    /// sidebar warms Ready covers at — a decode cached at another size is a
    /// different entry and would not spare this row its placeholder.
    static let coverPointSize: CGFloat = 44

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    /// Non-nil exactly when `row.selectable`. Passed in rather than read off
    /// `row` again so the list content is the one place selection state
    /// (`UiStore`) meets the row.
    let selection: Binding<Bool>?
    let isGroupMember: Bool
    let onSkip: (_ skipped: Bool) -> Void
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void

    @Environment(OutboxStore.self)
    private var outboxStore

    init(
        row: BridgeTriageRow,
        coverContent: ImageContent?,
        selection: Binding<Bool>?,
        isGroupMember: Bool,
        onSkip: @escaping (_ skipped: Bool) -> Void,
        onReleaseDecision:
            @escaping (
                _ key: BridgeFolderReleaseDecisionKey,
                _ decision: BridgeFolderReleaseDecision
            ) -> Void = { _, _ in }
    ) {
        self.row = row
        self.coverContent = coverContent
        self.selection = selection
        self.isGroupMember = isGroupMember
        self.onSkip = onSkip
        self.onReleaseDecision = onReleaseDecision
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            rowContent
            checkboxControl
                .padding(.top, 10)
                .padding(.leading, 9)
        }
        .padding(
            ImportListHierarchyLayout.insets(
                isGroupMember: isGroupMember
            )
        )
        .opacity(isPending ? 0.6 : 1)
        .contentShape(Rectangle())
        .contextMenu {
            if let skipAction = row.skipAction {
                switch skipAction {
                case .skip:
                    Button("Skip") { onSkip(true) }
                case .unskip:
                    Button("Unskip") { onSkip(false) }
                }
                Divider()
            }
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: row.candidateKey)
            }
            // A folder read as one release is this row and nothing else, so
            // its row is the only place left to say otherwise. A folder read
            // as several is a group of rows, and its header carries that
            // choice — a row is a release, not a place to answer a question
            // about the folder holding it.
            ForEach(combinedBoundaries, id: \.key) { boundary in
                Divider()
                Button("Keep as Separate Releases") {
                    onReleaseDecision(
                        boundary.key,
                        .keepAsSeparateReleases
                    )
                }
            }
        }
    }

    private var rowContent: some View {
        HStack(alignment: .top, spacing: 10) {
            Color.clear.frame(width: 18)
            cover
            meta
            Spacer(minLength: 4)
            trailing
                .padding(.top, 2)
        }
        .padding(.vertical, 7)
        .padding(.leading, 9)
        .padding(.trailing, 10)
    }

    /// The folders this row is the whole of, read as one release. Each offers
    /// to be read as several again.
    private var combinedBoundaries: [BridgeResolvedFolderReleaseBoundary] {
        row.resolvedBoundaries.filter(isCombined)
    }

    private var isPending: Bool {
        if case .needsYou(_, .stillIdentifying) = row.placement {
            return true
        }
        return false
    }

    // MARK: - Leading

    @ViewBuilder
    private var checkboxControl: some View {
        if let selection {
            Toggle(isOn: selection) {}
                .labelsHidden()
                .toggleStyle(.checkbox)
                .controlSize(.small)
                .frame(width: 18, height: 18)
                .padding(.top, 3)
        }
        else {
            Color.clear.frame(width: 18)
        }
    }

    private var cover: some View {
        ImageView(
            content: coverContent,
            pointSize: Self.coverPointSize
        )
        .frame(width: Self.coverPointSize, height: Self.coverPointSize)
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    // MARK: - Meta

    /// The list projection owns the persisted draft summary, so it remains
    /// visible independently of selection.
    private var releaseSummary: ImportReleaseSummary? {
        ImportReleaseSummary(row: row)
    }

    @ViewBuilder
    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let releaseSummary {
                ImportReleaseSummaryView(
                    summary: releaseSummary,
                    style: .sidebar
                )
            }
            else {
                folderTitle
            }
            stateLine
            if let progress = cloudUploadObservation?.progressBar {
                ProgressTrackBar(
                    progress: progress.fraction,
                    trackHeight: 3
                )
                .padding(.top, 7)
            }
        }
    }

    private var folderTitle: some View {
        HStack(spacing: 5) {
            Image(systemName: "folder")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            Text(row.folderName)
                .font(.system(size: 14, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    @ViewBuilder
    private var stateLine: some View {
        // A running import is the one line that changes by the second, so it
        // subscribes to the candidate-runtime signal at this leaf.
        if case .importing = row.importStatus {
            ImportProgressLine(key: row.candidateKey)
        }
        else if let statusLine {
            Text(statusLine)
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.top, 1)
        }
    }

}

/// The row's metadata and trailing column. In an extension so the view's body
/// and the layout it composes stay readable as one piece.
extension TriageRowView {
    /// State that belongs below the release summary: a disagreement, the
    /// still-identifying phase, or an import failure.
    private var statusLine: String? {
        switch row.placement {
        case .pending:
            return nil
        case .ready:
            return nil
        case .skipped:
            return nil
        case .needsYou(let group, let reason):
            switch reason {
            case .stillIdentifying(let phase):
                return phase.localizedText
            case .disagreement(let needsYou):
                switch group {
                case .alreadyInLibrary:
                    return nil
                case .pickAPressing:
                    return nil
                case .noMatch:
                    return nil
                default:
                    return needsYou.localizedText
                }
            }
        case .importing, .failed, .done:
            return importStatusLine
        }
    }

    private var importStatusLine: String? {
        switch row.importStatus {
        case .importing:
            return nil
        case .complete, nil:
            return nil
        case .error(let error):
            return error.displayLine
        }
    }

    // MARK: - Trailing

    @ViewBuilder
    private var trailing: some View {
        switch row.placement {
        case .pending:
            EmptyView()
        case .ready:
            EmptyView()
        case .needsYou(let group, let reason):
            needsYouTrailing(group: group, reason: reason)
        case .importing:
            ProgressView().controlSize(.small)
        case .failed, .done:
            importTrailing
        case .skipped:
            EmptyView()
        }
    }

    @ViewBuilder
    private func needsYouTrailing(
        group: BridgeNeedsYouGroup,
        reason: BridgeNeedsYouReason
    ) -> some View {
        switch reason {
        case .stillIdentifying(let phase):
            if phase == .running {
                ProgressView().controlSize(.small)
            }
            else {
                trailingIcon("clock", tint: .secondary)
            }
        case .disagreement(let needsYou):
            switch group {
            case .pickAPressing:
                chip(needsYou.localizedText, tint: .orange)
            case .alreadyInLibrary:
                chip(needsYou.localizedText, tint: .blue)
            case .countsOrLengthsDisagree:
                trailingIcon("questionmark.circle", tint: .orange)
            case .noMatch:
                EmptyView()
            case .stillIdentifying:
                EmptyView()
            }
        }
    }

    /// What a row past the point of being asked anything shows: the running
    /// import's spinner, the failure's tag, or the completed import's mark and
    /// its cloud transition.
    @ViewBuilder
    private var importTrailing: some View {
        switch row.importStatus {
        case .importing:
            ProgressView().controlSize(.small)
        case .complete:
            if case .active = cloudUploadObservation {
                // Still going up to the cloud — the same arrow the storage
                // queue marks an active upload with, and nothing else: the
                // release is in the library either way.
                trailingIcon("arrow.up.circle", tint: .secondary)
            }
            else {
                EmptyView()
            }
        case .error:
            chip(String(localized: "Failed"), tint: .red)
        case nil:
            // Already imported from a previous session (content-hash match),
            // so there is no in-session status to read — the fact is the
            // same, so the glyph is.
            trailingIcon("checkmark.circle.fill", tint: .green)
        }
    }

    /// The imported release's cloud transition, where the outbox holds one.
    /// A release with nothing queued is absent from the outbox, which is what
    /// "the import is done" reads as here.
    private var cloudUploadObservation: UploadObservation? {
        guard case .complete(let releaseId, _) = row.importStatus else {
            return nil
        }
        return outboxStore.persistedUploadObservation(forRelease: releaseId)
    }

    private func trailingIcon<S: ShapeStyle>(_ systemName: String, tint: S)
        -> some View
    {
        Image(systemName: systemName)
            .font(.caption)
            .foregroundStyle(tint)
    }

    private func chip(_ text: String, tint: Color) -> some View {
        Text(text)
            .font(.system(size: 10.5, design: .monospaced))
            .foregroundStyle(tint)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(
                RoundedRectangle(cornerRadius: 5).fill(tint.opacity(0.14))
            )
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Triage rows") {
        let importStore = ImportStore()
        VStack(alignment: .leading, spacing: 0) {
            TriageRowView(
                row: PreviewData.triageRowReady,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowReady
                ),
                selection: .constant(true),
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowStillIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowStillIdentifying
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowDoneImported
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowFailed
                ),
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
        }
        .padding()
        .frame(width: 340)
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }
#endif

/// Whether a settled reading is "this folder is one release" — the only one a
/// row can offer to reverse, because a folder read as several releases is a
/// group of rows and its header carries that choice.
func isCombined(_ boundary: BridgeResolvedFolderReleaseBoundary) -> Bool {
    if case .combineAsOneRelease = boundary.decision {
        return true
    }
    return false
}
