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
    static let coverPointSize: CGFloat = 50

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    /// The imported release's cloud transition, resolved by the list owner
    /// that already observes the outbox.
    let uploadObservation: UploadObservation?
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

    init(
        row: BridgeTriageRow,
        coverContent: ImageContent?,
        uploadObservation: UploadObservation?,
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
        self.uploadObservation = uploadObservation
        self.selection = selection
        self.isGroupMember = isGroupMember
        self.onSkip = onSkip
        self.onReleaseDecision = onReleaseDecision
    }

    var body: some View {
        rowContent
            .groupMemberRail(isGroupMember)
            .opacity(identificationInProgress ? 0.6 : 1)
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
        HStack(alignment: .center, spacing: 10) {
            cover
            meta
            Spacer(minLength: 4)
            trailing
            checkboxControl
        }
        .padding(.vertical, 6)
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
    }

    /// The folders this row is the whole of, read as one release. Each offers
    /// to be read as several again.
    private var combinedBoundaries: [BridgeResolvedFolderReleaseBoundary] {
        row.resolvedBoundaries.filter(isCombined)
    }

    private var identificationInProgress: Bool {
        if case .identification(let status) = row.placement {
            if case .finalizationFailed = status {
                return false
            }
            return true
        }
        return false
    }

    // MARK: - Leading

    /// Trailing, and only on rows that can join the bulk import: a row with
    /// nothing to select reserves nothing.
    @ViewBuilder
    private var checkboxControl: some View {
        if let selection {
            Toggle(isOn: selection) {}
                .labelsHidden()
                .toggleStyle(TriageCheckboxToggleStyle())
        }
    }

    /// The matched release's cover, or the image placeholder when there is
    /// none yet — the tile keeps every row's text starting at one x whether
    /// or not there is art.
    private var cover: some View {
        ImageView(
            content: coverContent,
            pointSize: Self.coverPointSize
        )
        .frame(width: Self.coverPointSize, height: Self.coverPointSize)
        .clipShape(RoundedRectangle(cornerRadius: 6))
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
            if let uploadObservation {
                ProgressLine(
                    uploadObservation.phaseText,
                    progress: uploadObservation.progressBar.fraction
                )
                .font(.system(size: 11.5))
            }
        }
    }

    private var folderTitle: some View {
        Text(row.folderName)
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
    }

    @ViewBuilder
    private var stateLine: some View {
        // A running import is the one line that changes by the second, so it
        // subscribes to the candidate-runtime signal at this leaf.
        if case .importing = row.importStatus {
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
extension TriageRowView {
    /// State that belongs below the release summary: a disagreement or an
    /// import failure. Identification activity belongs to its trailing
    /// indicator's tooltip.
    private var statusLine: String? {
        switch row.placement {
        case .pending:
            return nil
        case .identification(let status):
            if case .finalizationFailed(let error) = status {
                return error.displayLine
            }
            return nil
        case .ready:
            return nil
        case .skipped:
            return nil
        case .needsYou(let reason):
            switch reason {
            case .alreadyInLibrary:
                return nil
            case .severalMatches:
                return nil
            case .noMatch, .nothingToLookUp:
                return nil
            case .lookupFailed:
                return nil
            case .trackCountDisagrees, .durationsDisagree,
                .sourceLengthsUnknown, .localDurationUnknown:
                return reason.localizedText
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
        case .identification(let status):
            identificationTrailing(status)
        case .ready:
            EmptyView()
        case .needsYou(let reason):
            needsYouTrailing(reason)
        case .importing:
            // The line under the title carries the bar; nothing trails it.
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

    @ViewBuilder
    private func needsYouTrailing(_ reason: BridgeNeedsYou) -> some View {
        switch reason {
        case .severalMatches:
            chip(reason.localizedText, tint: .orange)
        case .alreadyInLibrary:
            chip(reason.localizedText, tint: .blue)
        case .trackCountDisagrees, .durationsDisagree,
            .sourceLengthsUnknown, .localDurationUnknown:
            trailingIcon("questionmark.circle", tint: .orange)
        case .lookupFailed:
            trailingIcon("exclamationmark.triangle.fill", tint: .orange)
                .help(reason.localizedText)
        case .noMatch, .nothingToLookUp:
            EmptyView()
        }
    }

    /// What a row past the point of being asked anything shows: the failure's
    /// tag, or the completed import's mark and its cloud transition. A running
    /// import's bar is on the line under the title.
    @ViewBuilder
    private var importTrailing: some View {
        switch row.importStatus {
        case .importing:
            EmptyView()
        case .complete:
            if case .active = uploadObservation {
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

/// The bulk-select checkbox, drawn by hand: the system checkbox is a filled
/// square that disappears against the dark list, so this one is an open
/// outline at rest and the accent tile with a mark when set.
private struct TriageCheckboxToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button {
            configuration.isOn = !configuration.isOn
        } label: {
            ZStack {
                RoundedRectangle(cornerRadius: 4)
                    .fill(Theme.accent)
                    .opacity(configuration.isOn ? 1 : 0)
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(.white.opacity(0.3), lineWidth: 1.5)
                    .opacity(configuration.isOn ? 0 : 1)
                Image(systemName: "checkmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Theme.background)
                    .opacity(configuration.isOn ? 1 : 0)
            }
            .frame(width: 18, height: 18)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
                uploadObservation: nil,
                selection: .constant(true),
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowIdentifying
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowDoneImported
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowFailed
                ),
                uploadObservation: nil,
                selection: nil,
                isGroupMember: false,
                onSkip: { _ in }
            )
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
