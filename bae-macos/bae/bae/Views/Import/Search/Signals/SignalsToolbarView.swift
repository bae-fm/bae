import BaeKit
import SwiftUI

/// The interactive signals toolbar shown above the identify results: the disc
/// ID, the barcode, and the catalog, side by side. A badge shows its value,
/// spins while its lookup runs, shows a result count when settled, and takes
/// itself in or out of the run on click — the catalog by picking one of the
/// numbers extracted from the candidate. The header carries the `Auto` action
/// (or an `Identifying…` spinner) and the `Search manually` /
/// `Skip identifying` escapes.
///
/// Core pre-shapes the whole badge list (`BridgeSignalsToolbar`); this view iterates
/// and renders — no domain logic here.
struct SignalsToolbarView: View {
    let toolbar: BridgeSignalsToolbar
    let onToggle: (BridgeSignalToggle) -> Void
    let onRerun: () -> Void
    let onSearchManually: () -> Void
    /// `nil` suppresses the "Skip identifying" pill — a CD carries no local
    /// data to seed a File Tags import until it's ripped.
    let onUseFileTags: (() -> Void)?

    /// The pipeline is still identifying while any badge is looking up. Drives
    /// the header spinner vs. the `Auto` link.
    private var isIdentifying: Bool {
        toolbar.signals.contains { $0.state == .lookingUp }
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
                        Text("Auto")
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
                if let onUseFileTags {
                    GhostPill(
                        icon: nil,
                        label: "Skip identifying",
                        action: onUseFileTags
                    )
                }
            }
        }
    }

    // MARK: - Badge row

    @ViewBuilder
    private var badgeRow: some View {
        // A wrapping row of the three badges. Badges stay whole units; the
        // value middle-truncates.
        WrappingHStack(spacing: 7, lineSpacing: 7) {
            ForEach(toolbar.signals) { signal in
                if signal.kind == .catalog {
                    CatalogSignalBadge(
                        signal: signal,
                        onChoose: { onToggle(.catalog(value: $0)) },
                    )
                }
                else {
                    SignalBadge(signal: signal, onToggle: { toggle(signal) })
                }
            }
        }
    }

    private func toggle(_ signal: BridgeToolbarSignal) {
        guard let toggle = BridgeSignalToggle(signal: signal) else {
            return
        }
        onToggle(toggle)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Both running") {
        SignalsToolbarView(
            toolbar: PreviewData.toolbarBothRunning,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onUseFileTags: {},
        )
        .frame(width: 720)
        .windowBackground()
    }

    #Preview("One settled, catalog confirms") {
        SignalsToolbarView(
            toolbar: PreviewData.toolbarOneSettled,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onUseFileTags: {},
        )
        .frame(width: 720)
        .windowBackground()
    }

    #Preview("Barcode excluded") {
        SignalsToolbarView(
            toolbar: PreviewData.toolbarBarcodeExcluded,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onUseFileTags: {},
        )
        .frame(width: 720)
        .windowBackground()
    }

    #Preview("Both signals matched") {
        SignalsToolbarView(
            toolbar: PreviewData.toolbarBothMatched,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onUseFileTags: {},
        )
        .frame(width: 720)
        .windowBackground()
    }

    #Preview("Skipped — no signals") {
        SignalsToolbarView(
            toolbar: PreviewData.toolbarSkippedNoSignals,
            onToggle: { _ in },
            onRerun: {},
            onSearchManually: {},
            onUseFileTags: nil,
        )
        .frame(width: 720)
        .windowBackground()
    }
#endif
