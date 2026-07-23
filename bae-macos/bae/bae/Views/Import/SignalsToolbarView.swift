import BaeKit
import SwiftUI

/// The interactive signals toolbar shown above the identify results. Each
/// identifying signal is a badge: the disc ID and barcode sit in the identity
/// zone on the left, then a `Refine` divider, then the catalog filter badges.
/// A badge shows its value, spins while its lookup runs, shows a result count
/// when settled, and toggles in/out of triangulation on click. The header
/// carries the `Re-run` action (or an `Identifying…` spinner) and the
/// `Search manually` / `Skip identifying` escapes.
///
/// Core pre-shapes the whole badge list (`BridgeSignalsToolbar`); this view iterates
/// and renders — no domain logic here.
struct SignalsToolbarView: View {
    let toolbar: BridgeSignalsToolbar
    let onToggle: (BridgeExcludedSignal) -> Void
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

    private func toggle(_ signal: BridgeToolbarSignal) {
        guard let excluded = BridgeExcludedSignal(signal: signal) else {
            return
        }
        onToggle(excluded)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Both running") {
        SignalsToolbarView(
            toolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                    origin: .discToc,
                    state: .lookingUp,
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "0123456789012",
                    origin: .artwork,
                    state: .lookingUp,
                    excluded: false
                ),
                BridgeToolbarSignal(
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
            toolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                    origin: .discToc,
                    state: .found(count: 3),
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "0123456789012",
                    origin: .artwork,
                    state: .lookingUp,
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .catalog,
                    role: .filter,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .confirms(count: 1),
                    excluded: false
                ),
                BridgeToolbarSignal(
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
            toolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false
                ),
                BridgeToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "0123456789012",
                    origin: .artwork,
                    state: .found(count: 4),
                    excluded: true
                ),
                BridgeToolbarSignal(
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
            toolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "Xx0Yy1Zz2Aa3Bb4Cc5Dd6Ee7-",
                    origin: .discToc,
                    state: .found(count: 2),
                    excluded: false
                ),
                BridgeToolbarSignal(
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
            toolbar: BridgeSignalsToolbar(signals: [
                BridgeToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: nil,
                    origin: .discToc,
                    state: .skipped,
                    excluded: false
                ),
                BridgeToolbarSignal(
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
#endif
