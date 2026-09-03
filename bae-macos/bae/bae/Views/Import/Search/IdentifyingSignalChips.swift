import BaeKit
import SwiftUI

/// The signals a run is looking up, side by side, while it looks them up.
///
/// It is the only place the pane spends a row on the chips: a settled verdict
/// says what identified the folder in one line, and Adjust holds the toggles.
/// While the run is going there is no verdict yet, so the chips are the
/// progress — each settling on its own count as its provider answers.
///
/// The catalog is the one chip that acts: it waits to be told which of the
/// folder's numbers to look up, and picking one re-runs with it.
struct IdentifyingSignalChips: View {
    let toolbar: BridgeSignalsToolbar
    let onToggle: (BridgeSignalToggle) -> Void

    /// Which chip's numbers are open, by signal id. Keyed rather than a flag:
    /// core sends one chip per catalog value it found, so a flag would open
    /// every one of them at once.
    @State
    private var pickingCatalog: String?

    var body: some View {
        HStack(spacing: 8) {
            ForEach(toolbar.signals) { signal in
                if signal.kind == .catalog {
                    catalogChip(signal)
                }
                else {
                    SignalChip(signal: signal)
                        .help(SignalBadgeStyle.stateLabel(for: signal))
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
    }

    /// The catalog's numbers, opened from the chip. A menu would render the
    /// chip as its own title and drop the count and the ring with it, so the
    /// numbers come up in a popover the chip keeps its shape under.
    private func catalogChip(_ signal: BridgeToolbarSignal) -> some View {
        Button {
            pickingCatalog = signal.id
        } label: {
            SignalChip(signal: signal, hasMenu: true)
        }
        .buttonStyle(.plain)
        .disabled(signal.options.isEmpty)
        .help(SignalBadgeStyle.stateLabel(for: signal))
        .popover(
            isPresented: Binding(
                get: { pickingCatalog == signal.id },
                set: { isOpen in pickingCatalog = isOpen ? signal.id : nil }
            ),
            arrowEdge: .bottom
        ) {
            CatalogOptionsList(options: signal.options) { value in
                pickingCatalog = nil
                onToggle(.catalog(value: value))
            }
            .padding(9)
            .frame(width: 280)
            .popoverEntrance(anchor: .top)
            .background { PopoverBehavior() }
        }
    }
}

/// One chip: what the signal is, the value it carries, and how its lookup is
/// going. A signal in the run carries the accent ring; one waiting to be told
/// which value to use reads as the plain control it is.
struct SignalChip: View {
    let signal: BridgeToolbarSignal
    /// Whether the chip opens a menu, so it draws the disclosure chevron.
    var hasMenu: Bool = false

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: SignalBadgeStyle.icon(for: signal.kind))
                .font(.system(size: 11))
                .foregroundStyle(isActive ? Theme.accent : .secondary)
            Text(SignalBadgeStyle.label(for: signal.kind))
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(isActive ? .primary : .secondary)
                .fixedSize()
            if let value = signal.value {
                Text(value)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 100, alignment: .leading)
            }
            status
            if hasMenu {
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 26)
        .background(
            isActive ? Theme.accent.opacity(0.05) : .clear,
            in: RoundedRectangle(cornerRadius: 7)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 7)
                .strokeBorder(
                    isActive
                        ? Theme.accent.opacity(0.4) : .white.opacity(0.12),
                    lineWidth: 1
                )
        )
        .opacity(signal.excluded ? 0.45 : 1)
    }

    /// A signal with nothing chosen for it — the catalog before a number is
    /// picked — is not in the run, so it reads like an excluded one.
    private var isActive: Bool {
        !signal.excluded && !SignalBadgeStyle.awaitingChoice(signal)
    }

    /// How the lookup is going. Both the spinner and the settled marker stay
    /// in the chip (opacity-swapped) so the row does not re-measure when one
    /// signal lands ahead of another.
    private var status: some View {
        ZStack {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
                .opacity(signal.state == .lookingUp ? 1 : 0)
            settledMarker
                .opacity(signal.state == .lookingUp ? 0 : 1)
        }
        .frame(minWidth: 19)
    }

    @ViewBuilder
    private var settledMarker: some View {
        if SignalBadgeStyle.awaitingChoice(signal) {
            // Nothing chosen: the count is how many there are to choose from.
            countCapsule(signal.options.count.formatted(), matched: false)
        }
        else {
            switch signal.state {
            case .found(let count):
                countCapsule(count.formatted(), matched: true)
            case .noMatch:
                countCapsule(0.formatted(), matched: false)
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 10))
                    .foregroundStyle(.orange)
            case .skipped, .lookingUp:
                Image(systemName: "minus")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func countCapsule(_ text: String, matched: Bool) -> some View {
        Text(text)
            .font(.system(size: 10.5, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(matched ? Color.green : Color.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(
                matched ? Color.green.opacity(0.14) : .white.opacity(0.06),
                in: RoundedRectangle(cornerRadius: 4)
            )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Identifying chips") {
        VStack(alignment: .leading, spacing: 0) {
            IdentifyingSignalChips(
                toolbar: PreviewData.toolbarBothRunning,
                onToggle: { _ in },
            )
            Divider()
            IdentifyingSignalChips(
                toolbar: PreviewData.toolbarCatalogChoices,
                onToggle: { _ in },
            )
            Divider()
            IdentifyingSignalChips(
                toolbar: PreviewData.toolbarSkippedNoSignals,
                onToggle: { _ in },
            )
        }
        .frame(width: 660)
        .windowBackground()
    }
#endif
