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
    /// Select this row — what a click does, and what "Import Anyway" and
    /// "Retry" alias: each opens the main pane exactly as selecting the row
    /// would.
    let onSelect: () -> Void
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
        onSelect: @escaping () -> Void,
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
        self.onSelect = onSelect
        self.onSkip = onSkip
        self.onReleaseDecision = onReleaseDecision
    }

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            checkbox
            cover
            VStack(alignment: .leading, spacing: 0) {
                meta
                if !rowActions.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(rowActions.indices, id: \.self) { index in
                            let action = rowActions[index]
                            Button(action.label, action: action.action)
                                .buttonStyle(
                                    RowActionPillStyle(isKey: action.isKey)
                                )
                        }
                    }
                    .padding(.top, 7)
                }
            }
            Spacer(minLength: 4)
            trailing
                .padding(.top, 2)
        }
        .padding(.vertical, 7)
        .padding(.leading, 9)
        .padding(.trailing, 10)
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
    private var checkbox: some View {
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

    private var meta: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                if ImportStore.titleIsFolderName(row) {
                    Image(systemName: "folder")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Text(ImportStore.displayTitle(row))
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            // A running import is the one line that changes by the second, so
            // it is a leaf of its own: it subscribes to the candidate-runtime
            // signal and redraws itself, leaving the rest of the row alone.
            if case .importing = row.importStatus {
                ImportProgressLine(key: row.candidateKey)
            }
            else if case .ready = row.placement {
                readyLines
            }
            else {
                if let subLine {
                    Text(subLine)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .padding(.top, 1)
                }
                if let progress = cloudUploadObservation?.progressBar {
                    ProgressTrackBar(
                        progress: progress.fraction,
                        trackHeight: 3
                    )
                    .padding(.top, 7)
                }
            }
        }
    }

    /// A Ready row says the same three things the pane header does — the
    /// title above, then who it is by, then the pressing — rather than
    /// packing all of it into one line.
    @ViewBuilder
    private var readyLines: some View {
        if let artist = row.matched?.artist {
            Text(artist)
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.top, 1)
        }
        if let pressingLine {
            Text(pressingLine)
                .font(.system(size: 11.5))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .padding(.top, 1)
        }
    }
}

/// The row's actions and its trailing column. In an extension so the view's
/// body and the layout it composes stay readable as one piece.
extension TriageRowView {
    /// The second line: the matched release's metadata, a disagreement
    /// sentence, the still-identifying phase, or an import failure —
    /// whichever `row` is actually saying.
    private var subLine: String? {
        switch row.placement {
        // A Ready row draws its own artist and pressing lines; `subLine` is
        // not asked for it.
        case .ready:
            return nil
        case .skipped:
            return metadataLine
        case .needsYou(let group, let reason):
            switch reason {
            case .stillIdentifying(let phase):
                return phase.localizedText
            case .disagreement(let needsYou):
                switch group {
                case .alreadyInLibrary:
                    // The pressing is settled here — exactly one match — so
                    // the row still leads with what it matched; the trailing
                    // chip states the disagreement instead.
                    return metadataLine
                case .pickAPressing:
                    // The pressing is exactly what's unsettled, so there is
                    // no metadata line yet — the lead match's artist is all
                    // that's known, and the trailing chip states the count.
                    return row.matched?.artist
                case .noMatch:
                    return nil
                default:
                    return needsYou.localizedText
                }
            }
        case .importing, .done:
            return importSubLine
        }
    }

    private var metadataLine: String? {
        guard let matched = row.matched else {
            return nil
        }
        var parts: [String] = []
        if let artist = matched.artist {
            parts.append(artist)
        }
        if let pressing = matched.pressing {
            if let year = pressing.year {
                parts.append(Int(year).formatted(.number.grouping(.never)))
            }
            if let format = pressing.format {
                parts.append(format)
            }
            if let trackCount = pressing.trackCount {
                parts.append(String(localized: "\(Int(trackCount)) tracks"))
            }
        }
        return parts.isEmpty ? nil : parts.joined(separator: " \u{b7} ")
    }

    /// The pressing on its own: `CD \u{b7} 1991 \u{b7} 10 tracks`, with
    /// whatever the source did not say left out.
    private var pressingLine: String? {
        guard let pressing = row.matched?.pressing else {
            return nil
        }
        var parts: [String] = []
        if let format = pressing.format {
            parts.append(format)
        }
        if let year = pressing.year {
            parts.append(Int(year).formatted(.number.grouping(.never)))
        }
        if let trackCount = pressing.trackCount {
            parts.append(String(localized: "\(Int(trackCount)) tracks"))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " \u{b7} ")
    }

    private var importSubLine: String? {
        switch row.importStatus {
        // A running import draws its own line; `subLine` is not asked for it.
        case .importing:
            return nil
        case .complete:
            if let statusText = cloudUploadObservation?.statusText {
                return statusText
            }
            return metadataLine
        case nil:
            return metadataLine
        case .error(let error):
            return error.displayLine
        }
    }

    // MARK: - Row actions

    private struct RowAction {
        let label: LocalizedStringKey
        let isKey: Bool
        let action: () -> Void
    }

    /// The pill row under the meta column: what this row is asking for right
    /// now. How the folder around it is read is not one of those — that lives
    /// on the group header, or in this row's own menu where the folder is
    /// this row.
    private var rowActions: [RowAction] {
        var actions: [RowAction] = []
        switch row.placement {
        case .needsYou(.alreadyInLibrary, .disagreement):
            actions = [
                RowAction(
                    label: "Import Anyway",
                    isKey: false,
                    action: onSelect
                )
            ]
        case .done:
            if case .error = row.importStatus {
                actions = [
                    RowAction(label: "Retry", isKey: true, action: onSelect),
                    RowAction(label: "Reveal in Finder", isKey: false) {
                        SystemActions.revealInFinder(path: row.candidateKey)
                    },
                ]
            }
        default:
            break
        }
        return actions
    }

    // MARK: - Trailing

    @ViewBuilder
    private var trailing: some View {
        switch row.placement {
        case .ready:
            chip(String(localized: "Ready"), tint: .green)
        case .needsYou(let group, let reason):
            needsYouTrailing(group: group, reason: reason)
        case .importing:
            ProgressView().controlSize(.small)
        case .done:
            doneTrailing
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

    @ViewBuilder
    private var doneTrailing: some View {
        switch row.importStatus {
        case .importing:
            ProgressView().controlSize(.small)
        case .complete:
            if case .active(let progress) = cloudUploadObservation {
                UploadActivityLabel(progress: progress)
                    .font(.caption)
            }
            else {
                trailingIcon("checkmark.circle.fill", tint: .green)
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

/// The row-action pill button style: a filled accent pill for the key action,
/// an outlined one for the rest.
private struct RowActionPillStyle: ButtonStyle {
    let isKey: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .medium))
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
            .foregroundStyle(isKey ? Theme.background : .secondary)
            .background(
                Capsule()
                    .fill(isKey ? Theme.accent : Color.clear)
            )
            .overlay(
                Capsule()
                    .strokeBorder(
                        isKey ? Color.clear : Color.secondary.opacity(0.4)
                    )
            )
            .opacity(configuration.isPressed ? 0.7 : 1)
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
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowPickAPressing,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowPickAPressing
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowSeveralMatchesFromSignals,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSeveralMatchesFromSignals
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowAlreadyInLibrary,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowAlreadyInLibrary
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowNoMatch,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowNoMatch
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowStillIdentifying,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowStillIdentifying
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowDoneImported,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowDoneImported
                ),
                selection: nil,
                onSelect: {},
                onSkip: { _ in }
            )
            TriageRowView(
                row: PreviewData.triageRowDoneFailed,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowDoneFailed
                ),
                selection: nil,
                onSelect: {},
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
