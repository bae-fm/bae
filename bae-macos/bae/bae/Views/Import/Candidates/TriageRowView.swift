import BaeKit
import SwiftUI

/// One triage row: cover, title, metadata, and status, with what is running
/// for the candidate read from the row's own live-state subscription. Its
/// height must not change on selection, which would shift every row below.
struct TriageRowView: View {
    /// The cover's edge in points; the sidebar preloads Pending's covers at
    /// this size, so it must match for the cached image to be used.
    static let coverPointSize = ThemeSize.rowArtwork

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    let isGroupMember: Bool
    /// What the row's menu offers, given what is running for it: the row's
    /// own actions, or the selection's when the row is part of a larger one.
    let menuOffers: (_ live: BridgeCandidateLiveState?) -> CandidateActionMenu
    let onPerform: (ImportCandidateActionOffer) -> Void

    init(
        row: BridgeTriageRow,
        coverContent: ImageContent?,
        isGroupMember: Bool,
        menuOffers:
            @escaping (_ live: BridgeCandidateLiveState?) ->
            CandidateActionMenu = { _ in .empty },
        onPerform: @escaping (ImportCandidateActionOffer) -> Void = { _ in }
    ) {
        self.row = row
        self.coverContent = coverContent
        self.isGroupMember = isGroupMember
        self.menuOffers = menuOffers
        self.onPerform = onPerform
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
                menuOffers: menuOffers(live),
                onPerform: onPerform
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
    /// What the row's menu offers, in the same order as the selection's pane.
    let menuOffers: CandidateActionMenu
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        rowContent
            .groupMemberRail(isGroupMember)
            .contentShape(Rectangle())
            .contextMenu {
                CandidateActionMenuItems(
                    menu: menuOffers,
                    onPerform: onPerform
                )
            }
    }

    private var rowContent: some View {
        HStack(alignment: .center, spacing: ThemeSpace.related) {
            cover
            meta
            Spacer(minLength: ThemeSpace.inline)
            trailing
        }
        .padding(.vertical, ThemeSpace.compact)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
    }

    // MARK: - Leading

    /// The matched release's cover, or a placeholder so every row's text lines
    /// up.
    private var cover: some View {
        ImageView(
            content: coverContent,
            pointSize: TriageRowView.coverPointSize
        )
        .frame(
            width: TriageRowView.coverPointSize,
            height: TriageRowView.coverPointSize
        )
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
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

    /// The saved draft's summary, which every reading but `unidentified` has.
    @ViewBuilder
    private var releaseSummary: some View {
        if let summary = ImportReleaseSummary(row: row) {
            ImportReleaseSummaryView(summary: summary, style: .sidebar) {
                RecordArrow(readFromRecord: row.reading.readFromRecord)
            }
        }
    }

    /// An unidentified row names its folder, as the main pane's heading does.
    private var folderLine: some View {
        HStack(spacing: ThemeSpace.compact) {
            Image(systemName: "folder")
                .themeIcon(.medium)
                .foregroundStyle(.secondary)
            Text(row.folderName)
                .themeText(.mono)
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            RecordArrow(readFromRecord: row.reading.readFromRecord)
                .layoutPriority(1)
        }
    }

    @ViewBuilder
    private var stateLine: some View {
        // A running import updates by the second, so only this leaf observes
        // its progress.
        if live?.importing == true {
            ImportProgressLine(key: row.candidateKey)
                .themeText(.detail)
        }
        else if let statusLine {
            Text(statusLine)
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

}

/// The row's status line and trailing column.
extension TriageRowContent {
    /// The line under the release summary: a failed import, or an
    /// identification result that failed to save.
    private var statusLine: String? {
        if case .finalizationFailed(let error) = live?.identification {
            return error.displayLine
        }
        switch row.placement {
        case .pending, .skipped:
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

    /// The trailing column, kept at its ideal width so the title truncates
    /// first.
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
                trailingIcon(
                    "exclamationmark.triangle.fill",
                    tint: Theme.warning
                )
                .help(line)
            }
            else {
                trailingIcon(
                    "exclamationmark.triangle.fill",
                    tint: Theme.warning
                )
            }
        }
    }

    /// A failed import's tag; a completed import's row is `ImportedRowView`.
    @ViewBuilder
    private var importTrailing: some View {
        switch row.importStatus {
        case .error:
            StatusChip("Failed", tone: .danger)
        case .complete, nil:
            EmptyView()
        }
    }

    private func trailingIcon<S: ShapeStyle>(_ systemName: String, tint: S)
        -> some View
    {
        Image(systemName: systemName)
            .themeIcon(.small)
            .foregroundStyle(tint)
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
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowPrefilledFromTags,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPrefilledFromTags
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedOnline,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedOnline
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifiedSeveralMatches,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifiedSeveralMatches
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentified,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentified
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifying
                ),
                isGroupMember: false
            )
            TriageRowView(
                row: PreviewData.triageRowFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowFailed
                ),
                isGroupMember: false
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
                    isGroupMember: false
                )
            }
            // The same row as the list's first, drawn as the selection draws
            // it: the whole text column goes white.
            TriageRowView(
                row: PreviewData.triageRowReadFromRecord,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowReadFromRecord
                ),
                isGroupMember: false
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
