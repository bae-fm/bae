import SwiftUI

/// The interactive signals toolbar shown above the identify results. Each
/// identifying signal is a badge: the disc ID and barcode sit in the identity
/// zone on the left, then a `Refine` divider, then the catalog filter badges.
/// A badge shows its value, spins while its lookup runs, shows a result count
/// when settled, and toggles in/out of triangulation on click. The header
/// carries the `Re-run` action (or an `Identifying…` spinner) and the
/// `Search manually` / `Skip identifying` escapes.
///
/// Core pre-shapes the whole badge list (`SignalsToolbar`); this view iterates
/// and renders — no domain logic here.
struct SignalsToolbarView: View {
    let toolbar: SignalsToolbar
    let onToggle: (ExcludedSignal) -> Void
    let onRerun: () -> Void
    let onSearchManually: () -> Void
    /// `nil` suppresses the "Skip identifying" pill — a CD carries no local
    /// data to seed an Unknown import until it's ripped.
    let onAddAsUnknown: (() -> Void)?

    /// The pipeline is still identifying while any identity badge is looking
    /// up. Drives the header spinner vs. the `Re-run` link.
    private var isIdentifying: Bool {
        toolbar.identity.contains { $0.state == .lookingUp }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            header
            badgeRow
        }
        .padding(.horizontal, 18)
        .padding(.top, 12)
        .padding(.bottom, 12)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: 10) {
            Text("Signals")
                .font(.system(size: 10.5, weight: .bold))
                .tracking(1.4)
                .textCase(.uppercase)
                .foregroundStyle(.tertiary)

            if isIdentifying {
                HStack(spacing: 7) {
                    ProgressView()
                        .controlSize(.small)
                        .scaleEffect(0.7)
                    Text("Identifying\u{2026}")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
            }
            else {
                Button(action: onRerun) {
                    HStack(spacing: 4) {
                        Image(systemName: "arrow.clockwise")
                            .font(.system(size: 11))
                        Text("Re-run")
                    }
                }
                .buttonStyle(.link)
                .font(.system(size: 12.5))
            }

            Spacer()

            HStack(spacing: 8) {
                GhostPill(
                    icon: "magnifyingglass",
                    label: "Search manually",
                    action: onSearchManually
                )
                if let onAddAsUnknown {
                    GhostPill(
                        icon: nil,
                        label: "Skip identifying",
                        action: onAddAsUnknown
                    )
                }
            }
        }
    }

    // MARK: - Badge row

    private var badgeRow: some View {
        // A wrapping row: identity badges, the Refine divider, then catalog
        // badges. Badges stay whole units; the value middle-truncates.
        WrappingHStack(spacing: 7, lineSpacing: 7) {
            ForEach(toolbar.identity) { signal in
                SignalBadge(signal: signal, onToggle: { toggle(signal) })
            }
            if !toolbar.filters.isEmpty {
                refineDivider
                ForEach(toolbar.filters) { signal in
                    SignalBadge(signal: signal, onToggle: { toggle(signal) })
                }
            }
        }
    }

    private var refineDivider: some View {
        HStack(spacing: 6) {
            Rectangle()
                .fill(.white.opacity(0.14))
                .frame(width: 1, height: 22)
            Text("Refine")
                .font(.system(size: 9.5, weight: .bold))
                .tracking(1)
                .textCase(.uppercase)
                .foregroundStyle(.quaternary)
        }
        .padding(.horizontal, 3)
    }

    private func toggle(_ signal: ToolbarSignal) {
        guard let excluded = ExcludedSignal(signal: signal) else {
            return
        }
        onToggle(excluded)
    }
}

// MARK: - WrappingHStack

/// A horizontal layout that wraps its subviews onto new lines when they don't
/// fit the proposed width — like flex-wrap. The badge row uses it so badges
/// stay whole units and the row flows onto multiple lines at narrow widths.
struct WrappingHStack: Layout {
    var spacing: CGFloat = 8
    var lineSpacing: CGFloat = 8

    func sizeThatFits(
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) -> CGSize {
        let maxWidth = proposal.width ?? .infinity
        let rows = layoutRows(subviews: subviews, maxWidth: maxWidth)
        let width = rows.map(\.width).max() ?? 0
        let height =
            rows.map(\.height).reduce(0, +)
            + lineSpacing * CGFloat(max(0, rows.count - 1))
        return CGSize(width: width, height: height)
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout Void
    ) {
        let rows = layoutRows(subviews: subviews, maxWidth: bounds.width)
        var y = bounds.minY
        for row in rows {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index]
                    .place(
                        at: CGPoint(x: x, y: y),
                        anchor: .topLeading,
                        proposal: ProposedViewSize(size)
                    )
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    /// One wrapped line: the subview indices on it plus its measured extent.
    private struct Row {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func layoutRows(subviews: Subviews, maxWidth: CGFloat) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let projected =
                row.indices.isEmpty
                ? size.width : row.width + spacing + size.width
            if !row.indices.isEmpty, projected > maxWidth {
                rows.append(row)
                row = Row()
            }
            if row.indices.isEmpty {
                row.width = size.width
            }
            else {
                row.width += spacing + size.width
            }
            row.height = max(row.height, size.height)
            row.indices.append(index)
        }
        if !row.indices.isEmpty {
            rows.append(row)
        }
        return rows
    }
}

// MARK: - GhostPill

/// A transparent ghost pill button — the toolbar's escape actions.
private struct GhostPill: View {
    let icon: String?
    let label: LocalizedStringKey
    let action: () -> Void

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let icon {
                    Image(systemName: icon)
                        .font(.system(size: 11))
                }
                Text(label)
                    .font(.system(size: 12.5, weight: .medium))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .foregroundStyle(hovering ? .primary : .secondary)
            .background(
                .white.opacity(hovering ? 0.05 : 0),
                in: RoundedRectangle(cornerRadius: 7)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

// MARK: - SignalBadge

/// One signal badge: a type icon, a label, the value (mono, middle-truncated),
/// and a trailing status. An active badge carries an accent ring; an excluded
/// badge dims and strikes through but stays in place. Hovering shows a popover
/// explaining the signal's origin, role, full value, state, and the
/// click-to-toggle hint.
private struct SignalBadge: View {
    let signal: ToolbarSignal
    let onToggle: () -> Void

    @State
    private var hovering = false

    /// A confirming catalog (a filter that matched a pressing) pins the badge
    /// with an accent ring + check.
    private var isPinned: Bool {
        if case .confirms(let count) = signal.state {
            return count > 0 && !signal.excluded
        }
        return false
    }

    /// The accent ring shows on an active (not excluded) badge. A pinned
    /// catalog gets a stronger ring (handled in the overlay).
    private var isActive: Bool {
        !signal.excluded
    }

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 7) {
                Image(systemName: SignalBadgeStyle.icon(for: signal.kind))
                    .font(.system(size: 12))
                    .foregroundStyle(iconColor)
                Text(SignalBadgeStyle.label(for: signal.kind))
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(.primary)
                    .strikethrough(signal.excluded, color: .secondary)
                if let value = signal.value {
                    Text(value)
                        .font(.system(size: 11.5, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .strikethrough(signal.excluded, color: .secondary)
                        .frame(maxWidth: 160, alignment: .leading)
                        .fixedSize(horizontal: false, vertical: true)
                }
                status
            }
            .padding(.leading, 9)
            .padding(.trailing, 8)
            .frame(height: 30)
            .background(badgeBackground)
            .overlay(badgeRing)
            .opacity(signal.excluded ? 0.45 : 1)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .popover(isPresented: $hovering, arrowEdge: .bottom) {
            SignalBadgePopover(signal: signal)
        }
    }

    private var iconColor: Color {
        if signal.excluded {
            return .secondary
        }
        return isPinned || isActive ? Theme.accent : .secondary
    }

    private var badgeBackground: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(Color.white.opacity(0.04))
    }

    private var badgeRing: some View {
        RoundedRectangle(cornerRadius: 8)
            .stroke(
                isPinned
                    ? Theme.accent
                    : (isActive
                        ? Theme.accent.opacity(0.35) : .white.opacity(0.08)),
                lineWidth: isPinned ? 1.2 : 1
            )
    }

    @ViewBuilder
    private var status: some View {
        if signal.excluded {
            // Excluded: a hollow off-dot with an X.
            Image(systemName: "xmark")
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(.tertiary)
                .frame(width: 14, height: 14)
                .overlay(Circle().stroke(.tertiary, lineWidth: 1.5))
        }
        else {
            switch signal.state {
            case .lookingUp:
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.6)
                    .frame(width: 16, height: 16)
            case .found(let count):
                countCapsule(count.formatted(), tone: .green)
            case .confirms(let count):
                if count > 0 {
                    Image(systemName: "checkmark")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(Theme.accent)
                        .frame(minWidth: 19, minHeight: 19)
                        .background(
                            Theme.accent.opacity(0.18),
                            in: RoundedRectangle(cornerRadius: 6)
                        )
                }
                else {
                    countCapsule(0.formatted(), tone: .muted)
                }
            case .noMatch:
                countCapsule(0.formatted(), tone: .muted)
            case .skipped:
                Image(systemName: "minus")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 19, height: 19)
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
                    .frame(width: 19, height: 19)
            }
        }
    }

    private enum StatusTone {
        case green
        case muted
    }

    private func countCapsule(_ text: String, tone: StatusTone) -> some View {
        Text(text)
            .font(.system(size: 11.5, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(tone == .green ? Color.green : Color.secondary)
            .frame(minWidth: 19, minHeight: 19)
            .padding(.horizontal, 6)
            .background(
                tone == .green
                    ? Color.green.opacity(0.14) : .white.opacity(0.05),
                in: RoundedRectangle(cornerRadius: 6)
            )
    }
}

// MARK: - SignalBadgePopover

/// The hover popover for a badge: where the signal came from, its role, the
/// full untruncated value, its current state, and the click-to-toggle hint.
private struct SignalBadgePopover: View {
    let signal: ToolbarSignal

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: SignalBadgeStyle.icon(for: signal.kind))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Text(
                    "\(SignalBadgeStyle.label(for: signal.kind)) · from \(SignalBadgeStyle.originLabel(for: signal.origin))"
                )
                .font(.system(size: 11, weight: .semibold))
                .tracking(0.5)
                .textCase(.uppercase)
                .foregroundStyle(.tertiary)
            }

            Text(SignalBadgeStyle.roleLabel(for: signal.role))
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)

            if let value = signal.value {
                Text(value)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }

            Divider()

            HStack(spacing: 6) {
                Circle()
                    .fill(SignalBadgeStyle.stateDotColor(for: signal))
                    .frame(width: 5, height: 5)
                Text(SignalBadgeStyle.stateLabel(for: signal))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(signal.excluded ? "Click to include" : "Click to exclude")
                    .font(.system(size: 11.5))
                    .foregroundStyle(Theme.accent)
            }
        }
        .padding(11)
        .frame(width: 246)
    }
}

// MARK: - SignalBadgeStyle

/// Presentation strings + colors for a badge, all derived from the badge's
/// pre-shaped fields. Kept as a caseless namespace so both the badge and its
/// popover render identically.
private enum SignalBadgeStyle {
    static func icon(for kind: SignalKind) -> String {
        switch kind {
        case .discId: "opticaldiscdrive"
        case .barcode: "barcode"
        case .catalog: "tag"
        }
    }

    static func label(for kind: SignalKind) -> String {
        switch kind {
        case .discId: String(localized: "Disc ID")
        case .barcode: String(localized: "Barcode")
        case .catalog: String(localized: "Catalog")
        }
    }

    static func originLabel(for origin: SignalOrigin) -> String {
        switch origin {
        case .discToc: String(localized: "Disc TOC")
        case .cueSheet: String(localized: "CUE sheet")
        case .artwork: String(localized: "Cover OCR")
        case .folderName: String(localized: "folder name")
        case .filename: String(localized: "file name")
        case .textFile: String(localized: "Text file")
        }
    }

    static func roleLabel(for role: SignalRole) -> String {
        switch role {
        case .identity: String(localized: "Identifies · finds releases")
        case .filter: String(localized: "Refines · narrows the match")
        }
    }

    static func stateLabel(for signal: ToolbarSignal) -> String {
        if signal.excluded {
            return String(localized: "Excluded from search")
        }
        switch signal.state {
        case .lookingUp:
            return signal.role == .filter
                ? String(localized: "Matching\u{2026}")
                : String(localized: "Looking up\u{2026}")
        case .found(let count):
            return String(localized: "\(Int(count)) releases")
        case .noMatch:
            return String(localized: "No releases matched")
        case .skipped:
            return signal.kind == .discId
                ? String(localized: "No disc layout")
                : String(localized: "No source to scan")
        case .failed(let failure):
            return failure.badgeLine
        case .confirms(let count):
            return count > 0
                ? String(localized: "Matches this pressing")
                : String(localized: "No matched pressing carries this catno")
        }
    }

    static func stateDotColor(for signal: ToolbarSignal) -> Color {
        if signal.excluded {
            return .secondary
        }
        switch signal.state {
        case .found(let count): return count > 0 ? .green : .orange
        case .confirms(let count): return count > 0 ? Theme.accent : .orange
        case .noMatch: return .orange
        case .failed: return .orange
        case .skipped: return .secondary
        case .lookingUp: return .secondary
        }
    }
}

// MARK: - Previews

#Preview("Both running") {
    SignalsToolbarView(
        toolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .lookingUp,
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 0),
                excluded: false
            ),
        ]),
        onToggle: { _ in },
        onRerun: {},
        onSearchManually: {},
        onAddAsUnknown: {},
    )
    .frame(width: 720)
    .windowBackground()
}

#Preview("One settled, catalog confirms") {
    SignalsToolbarView(
        toolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 3),
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .lookingUp,
                excluded: false
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 1),
                excluded: false
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "A2 16018",
                origin: .textFile,
                state: .confirms(count: 0),
                excluded: false
            ),
        ]),
        onToggle: { _ in },
        onRerun: {},
        onSearchManually: {},
        onAddAsUnknown: {},
    )
    .frame(width: 720)
    .windowBackground()
}

#Preview("Barcode excluded") {
    SignalsToolbarView(
        toolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "0123456789012",
                origin: .artwork,
                state: .found(count: 4),
                excluded: true
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 0),
                excluded: false
            ),
        ]),
        onToggle: { _ in },
        onRerun: {},
        onSearchManually: {},
        onAddAsUnknown: {},
    )
    .frame(width: 720)
    .windowBackground()
}

#Preview("Conflict — both matched") {
    SignalsToolbarView(
        toolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "5051961234567",
                origin: .artwork,
                state: .found(count: 3),
                excluded: false
            ),
        ]),
        onToggle: { _ in },
        onRerun: {},
        onSearchManually: {},
        onAddAsUnknown: {},
    )
    .frame(width: 720)
    .windowBackground()
}

#Preview("Skipped — no signals") {
    SignalsToolbarView(
        toolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: nil,
                origin: .discToc,
                state: .skipped,
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: nil,
                origin: .artwork,
                state: .skipped,
                excluded: false
            ),
        ]),
        onToggle: { _ in },
        onRerun: {},
        onSearchManually: {},
        onAddAsUnknown: nil,
    )
    .frame(width: 720)
    .windowBackground()
}
