import BaeKit
import SwiftUI

/// One triage row: cover, title, metadata, and status. Selection belongs to
/// the surrounding list.
///
/// A row does not change height on selection: the folder it came from is the
/// main pane's to state, and a row that grows on selection shifts every row
/// under it.
///
/// What the list delivers is what the tables say; what is running for the
/// candidate — a run queued or in flight, an import that owns it — and the
/// commands the row offers with it are the row's own subscription's, read
/// here and drawn by `TriageRowContent`.
struct TriageRowView: View {
    /// The cover's edge, in points. Named because it is also the size the
    /// sidebar warms Ready covers at — a decode cached at another size is a
    /// different entry and would not spare this row its placeholder.
    static let coverPointSize: CGFloat = 50

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    let isGroupMember: Bool
    let onReveal: () -> Void
    let onSkip: (_ skipped: Bool) -> Void
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void

    init(
        row: BridgeTriageRow,
        coverContent: ImageContent?,
        isGroupMember: Bool,
        onReveal: @escaping () -> Void,
        onSkip: @escaping (_ skipped: Bool) -> Void,
        onReleaseDecision:
            @escaping (
                _ key: BridgeFolderReleaseDecisionKey,
                _ decision: BridgeFolderReleaseDecision
            ) -> Void = { _, _ in }
    ) {
        self.row = row
        self.coverContent = coverContent
        self.isGroupMember = isGroupMember
        self.onReveal = onReveal
        self.onSkip = onSkip
        self.onReleaseDecision = onReleaseDecision
    }

    var body: some View {
        CandidateLiveStateReader(
            key: row.candidateKey,
            basis: row.actionBasis
        ) { live in
            TriageRowContent(
                row: row,
                live: live,
                coverContent: coverContent,
                isGroupMember: isGroupMember,
                onReveal: onReveal,
                onSkip: onSkip,
                onReleaseDecision: onReleaseDecision
            )
        }
    }
}

/// A triage row drawn from its row and its live state.
struct TriageRowContent: View {
    let row: BridgeTriageRow
    /// `nil` until the row's subscription has answered.
    let live: BridgeCandidateLiveState?
    let coverContent: ImageContent?
    let isGroupMember: Bool
    let onReveal: () -> Void
    let onSkip: (_ skipped: Bool) -> Void
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void

    var body: some View {
        rowContent
            .groupMemberRail(isGroupMember)
            .contentShape(Rectangle())
            .contextMenu {
                if let actions = live?.actions,
                    actions.contains(.skip) || actions.contains(.restore)
                {
                    if actions.contains(.skip) {
                        Button("Skip") { onSkip(true) }
                    }
                    if actions.contains(.restore) {
                        Button("Unskip") { onSkip(false) }
                    }
                    Divider()
                }
                Button("Reveal in Finder", action: onReveal)
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
        HStack(alignment: .center, spacing: 10) {
            cover
            meta
            Spacer(minLength: 4)
            trailing
        }
        .padding(.vertical, 6)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
    }

    /// The folders this row is the whole of, read as one release. Each offers
    /// to be read as several again.
    private var combinedBoundaries: [BridgeResolvedFolderReleaseBoundary] {
        row.resolvedBoundaries.filter(isCombined)
    }

    // MARK: - Leading

    /// The matched release's cover, or the image placeholder when there is
    /// none yet — the tile keeps every row's text starting at one x whether
    /// or not there is art.
    private var cover: some View {
        ImageView(
            content: coverContent,
            pointSize: TriageRowView.coverPointSize
        )
        .frame(
            width: TriageRowView.coverPointSize,
            height: TriageRowView.coverPointSize
        )
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }

    // MARK: - Meta

    @ViewBuilder
    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            switch row.reading {
            case .unidentified:
                folderLine
            case .prefilled, .identified:
                releaseSummary
            }
            stateLine
        }
    }

    /// The list projection owns the persisted draft summary, so it remains
    /// visible independently of selection. Every reading but `unidentified`
    /// is a row that carries one.
    @ViewBuilder
    private var releaseSummary: some View {
        if let summary = ImportReleaseSummary(row: row) {
            ImportReleaseSummaryView(summary: summary, style: .sidebar) {
                RecordArrow(readFromRecord: row.reading.readFromRecord)
            }
        }
    }

    /// A row nothing has been written about names the folder it came from,
    /// as the main pane's heading does.
    private var folderLine: some View {
        HStack(spacing: 6) {
            Image(systemName: "folder")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
            Text(row.folderName)
                .font(.system(size: 12.5, design: .monospaced))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            RecordArrow(readFromRecord: row.reading.readFromRecord)
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    private var stateLine: some View {
        // A running import is the one line that changes by the second, so it
        // subscribes to the candidate-runtime signal at this leaf.
        if live?.importing == true {
            ImportProgressLine(key: row.candidateKey)
                .font(.system(size: 11.5))
        }
        else if let statusLine {
            Text(statusLine)
                .font(.system(size: 11.5))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.top, 0)
        }
    }

}

/// The row's metadata and trailing column. In an extension so the view's body
/// and the layout it composes stay readable as one piece.
extension TriageRowContent {
    /// State that belongs below the release summary: an import failure, or a
    /// write of an identification result that failed. What a Ready check found
    /// is the pane's to state, beside the Import it bears on; identification
    /// activity belongs to the trailing indicator's tooltip.
    private var statusLine: String? {
        if case .finalizationFailed(let error) = live?.identification {
            return error.displayLine
        }
        switch row.placement {
        case .pending, .ready, .skipped, .needsYou:
            return nil
        case .failed, .done:
            return importStatusLine
        }
    }

    private var importStatusLine: String? {
        switch row.importStatus {
        case .complete, nil:
            return nil
        case .error(let error):
            return error.displayLine
        }
    }

    // MARK: - Trailing

    /// What the row ends with. Kept at its ideal width: the release's title and
    /// artist truncate before a tag does, since a tag is already as short as it
    /// gets. What the row states about its release is the title line's arrow,
    /// not a column of its own.
    ///
    /// A run in flight takes the column rather than sitting beside it: while a
    /// run is going there is nothing to answer, and the answer being written
    /// is about to replace whatever the column said. A running import leaves
    /// it empty — the line under the title carries the bar. Otherwise the
    /// import says what it has to. What an identification result asks is the
    /// pane's to state, never the row's.
    private var trailing: some View {
        Group {
            if let identification = live?.identification {
                identificationTrailing(identification)
            }
            else if live?.importing == true {
                EmptyView()
            }
            else {
                placementTrailing
            }
        }
        .fixedSize()
    }

    @ViewBuilder
    private var placementTrailing: some View {
        switch row.placement {
        case .pending:
            EmptyView()
        case .ready:
            EmptyView()
        case .needsYou:
            // The question is the pane's to state.
            EmptyView()
        case .failed, .done:
            importTrailing
        case .skipped:
            EmptyView()
        }
    }

    @ViewBuilder
    private func identificationTrailing(
        _ status: BridgeIdentificationStatus
    ) -> some View {
        switch status {
        case .queued:
            trailingIcon("clock", tint: .secondary)
                .help(String(localized: "Waiting to be identified"))
        case .running, .finalizing:
            ProgressView()
                .controlSize(.small)
                .help(String(localized: "Identifying\u{2026}"))
        case .finalizationFailed(let error):
            if let line = error.displayLine {
                trailingIcon("exclamationmark.triangle.fill", tint: .orange)
                    .help(line)
            }
            else {
                trailingIcon("exclamationmark.triangle.fill", tint: .orange)
            }
        }
    }

    /// What a failed import's row shows: the failure's tag. A completed
    /// import's row is the library release it became, which `ImportedRowView`
    /// draws.
    @ViewBuilder
    private var importTrailing: some View {
        switch row.importStatus {
        case .error:
            chip(String(localized: "Failed"), tint: .red)
        case .complete, nil:
            EmptyView()
        }
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
                row: PreviewData.triageRowUnidentified,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowUnidentified
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowPrefilledFromTags,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPrefilledFromTags
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedOnline,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedOnline
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedSeveralMatches,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedSeveralMatches
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowReady,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowReady
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifying
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowFailed
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
        }
        .padding()
        .frame(width: 340)
        .environment(PreviewData.artImageStore())
        .candidateReaderPreviewEnvironment()
        .windowBackground()
    }

    #Preview("Where the draft was read from") {
        let importStore = ImportStore()
        let rows = [
            PreviewData.triageRowReadFromRecord,
            PreviewData.triageRowNotReadFromRecord,
        ]
        return VStack(alignment: .leading, spacing: 0) {
            ForEach(rows, id: \.candidateKey) { row in
                TriageRowView(
                    row: row,
                    coverContent: importStore.sidebarCover(for: row),
                    isGroupMember: false,
                    onReveal: {},
                    onSkip: { _ in }
                )
            }
            // The same row as the list's first, drawn as the selection draws
            // it: the whole text column goes white.
            TriageRowView(
                row: PreviewData.triageRowReadFromRecord,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowReadFromRecord
                ),
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(\.backgroundProminence, .increased)
            .background(Color.accentColor)
        }
        .padding()
        .frame(width: 340)
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
