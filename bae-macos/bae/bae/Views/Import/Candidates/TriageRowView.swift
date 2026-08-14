import BaeKit
import SwiftUI

/// One triage row: an optional bulk-select checkbox, the matched release's
/// cover, its title/metadata, and trailing evidence or status. Every decision
/// about what the row shows — which tab, which group, whether it takes a
/// checkbox — is `row`'s, read off `BridgeTriageRow`; this only renders it.
///
/// Every row is the same two lines and the same height whether or not it is
/// the selected one: the folder it came from is the main pane's to state, and
/// a row that grows on selection shifts every row under it.
struct TriageRowView: View {
    /// The cover's edge, in points. Named because it is also the size the
    /// sidebar warms Ready covers at — a decode cached at another size is a
    /// different entry and would not spare this row its placeholder.
    static let coverPointSize: CGFloat = 44

    let row: BridgeTriageRow
    let coverContent: ImageContent?
    /// Non-nil exactly when `row.selectable` — the checkbox's checked state
    /// and toggle. Passed in rather than read off `row` again so the list
    /// content is the one place selection state (`UiStore`) meets the row.
    let selection: (isSelected: Bool, toggle: () -> Void)?
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
        selection: (isSelected: Bool, toggle: () -> Void)?,
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
            if canSkip {
                Button(skipped ? "Unskip" : "Skip") { onSkip(!skipped) }
                Divider()
            }
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: row.candidateKey)
            }
            if let combineKey = row.combineAncestorKey {
                Divider()
                Button("Combine as One Release") {
                    onReleaseDecision(combineKey, .combineAsOneRelease)
                }
            }
            ForEach(
                row.resolvedBoundaries,
                id: \.key
            ) { boundary in
                Divider()
                switch boundary.decision {
                case .combineAsOneRelease:
                    Button("Keep as Separate Releases") {
                        onReleaseDecision(
                            boundary.key,
                            .keepAsSeparateReleases
                        )
                    }
                case .keepAsSeparateReleases:
                    Button("Combine as One Release") {
                        onReleaseDecision(
                            boundary.key,
                            .combineAsOneRelease
                        )
                    }
                }
            }
        }
    }

    private var skipped: Bool {
        row.placement == .skipped
    }

    /// Skipping is a decision about a candidate nobody has committed to yet:
    /// an import already running or already finished is past the point where
    /// setting it aside means anything.
    private var canSkip: Bool {
        switch row.placement {
        case .importing, .done: false
        case .ready, .needsYou, .skipped: true
        }
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
            Button(action: selection.toggle) {
                RoundedRectangle(cornerRadius: 5)
                    .fill(
                        selection.isSelected
                            ? Theme.accent : Color.clear
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 5)
                            .strokeBorder(
                                selection.isSelected
                                    ? Color.clear : Color.secondary
                            )
                    )
                    .overlay {
                        if selection.isSelected {
                            Image(systemName: "checkmark")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(Theme.background)
                        }
                    }
                    .frame(width: 18, height: 18)
            }
            .buttonStyle(.plain)
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
            if let subLine {
                Text(subLine)
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .padding(.top, 1)
            }
            if case .importing(let percent, _) = row.importStatus {
                ProgressTrackBar(
                    progress: Double(percent) / 100,
                    trackHeight: 3
                )
                .padding(.top, 7)
            }
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
        case .ready, .skipped:
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

    private var importSubLine: String? {
        switch row.importStatus {
        case .importing(let percent, let step):
            let phaseText =
                step?.localizedText ?? String(localized: "Importing\u{2026}")
            // The percent renders through `.formatted(.percent)` before it's
            // interpolated, so the localized template only ever sees two
            // string slots — a literal `%` next to a format specifier is
            // ambiguous to hand-author as a catalog key.
            let percentText = (Double(percent) / 100).formatted(.percent)
            return String(localized: "\(phaseText) \u{b7} \(percentText)")
        case .complete, nil:
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

    /// The pill row under the meta column. Placement-specific commands and
    /// persisted folder-boundary reversal commands share the same renderer.
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
        for boundary in row.resolvedBoundaries {
            switch boundary.decision {
            case .combineAsOneRelease:
                actions.append(
                    RowAction(
                        label: "Keep as Separate Releases",
                        isKey: false
                    ) {
                        onReleaseDecision(
                            boundary.key,
                            .keepAsSeparateReleases
                        )
                    }
                )
            case .keepAsSeparateReleases:
                actions.append(
                    RowAction(label: "Combine as One Release", isKey: false) {
                        onReleaseDecision(
                            boundary.key,
                            .combineAsOneRelease
                        )
                    }
                )
            }
        }
        return actions
    }

    // MARK: - Trailing

    @ViewBuilder
    private var trailing: some View {
        switch row.placement {
        case .ready:
            if let matched = row.matched {
                readyTrailing(matched.evidence)
            }
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

    private func readyTrailing(_ evidence: BridgeMatchEvidence) -> some View {
        VStack(alignment: .trailing, spacing: 4) {
            Text(verbatim: evidence.source.shortCode)
                .font(.system(size: 10, weight: .medium, design: .monospaced))
                .foregroundStyle(.tertiary)
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(
                    RoundedRectangle(cornerRadius: 4)
                        .fill(Color.secondary.opacity(0.12))
                )
            if let signal = evidence.signal {
                signalIcon(signal)
                    .help(
                        "\(signal.localizedLabel) \u{b7} \(evidence.source.displayName)"
                    )
            }
        }
    }

    private func signalIcon(_ signal: BridgeMatchedSignal) -> some View {
        let systemName =
            switch signal {
            case .discId: "opticaldiscdrive"
            case .barcode: "barcode"
            }
        return Image(systemName: systemName)
            .font(.caption)
            .foregroundStyle(.secondary)
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
            case .signalsDisagree:
                chip(String(localized: "Conflict"), tint: .red)
            case .alreadyInLibrary:
                chip(needsYou.localizedText, tint: .blue)
            case .countsOrLengthsDisagree:
                trailingIcon("questionmark.circle", tint: .orange)
            case .noMatch:
                Button(action: onSelect) {
                    chip(String(localized: "Search manually"), tint: .secondary)
                }
                .buttonStyle(.plain)
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
        case .complete(let releaseId, _):
            if outboxStore.progress(forRelease: releaseId) != nil {
                trailingIcon("icloud.and.arrow.up", tint: .secondary)
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

extension BridgeMetadataSource {
    /// Brand names stay literal — never translated, matching every other
    /// provider name in this app's chrome.
    fileprivate var displayName: String {
        switch self {
        case .musicBrainz: "MusicBrainz"
        case .discogs: "Discogs"
        }
    }

    /// The row's compact trailing badge. Not a catalog key — an abbreviation
    /// of a literal brand name is still the brand's name, not prose.
    fileprivate var shortCode: String {
        switch self {
        case .musicBrainz: "MB"
        case .discogs: "DC"
        }
    }
}

extension BridgeMatchedSignal {
    fileprivate var localizedLabel: String {
        switch self {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "Barcode")
        }
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
                selection: (isSelected: true, toggle: {}),
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
                row: PreviewData.triageRowSignalsConflict,
                coverContent: importStore.sidebarCover(
                    for: PreviewData.triageRowSignalsConflict
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
        .windowBackground()
    }
#endif
