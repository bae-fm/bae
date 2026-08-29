import BaeKit
import SwiftUI

/// The chip every signal badge is drawn as: a type icon, a label, the value
/// (mono, middle-truncated), and a trailing status. An active badge carries an
/// accent ring; an unchecked one dims and strikes through but stays in place,
/// so the row's layout holds steady.
struct SignalBadgeChip: View {
    let signal: BridgeToolbarSignal

    var body: some View {
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
                    .textSelection(.enabled)
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

    /// A signal with nothing chosen for it — the catalog before the user picks
    /// a number — is not in the run, so it reads like an unchecked one.
    private var isActive: Bool {
        !signal.excluded && !SignalBadgeStyle.awaitingChoice(signal)
    }

    private var iconColor: Color {
        isActive ? Theme.accent : .secondary
    }

    private var badgeBackground: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(Color.white.opacity(0.04))
    }

    private var badgeRing: some View {
        RoundedRectangle(cornerRadius: 8)
            .stroke(
                isActive ? Theme.accent.opacity(0.35) : .white.opacity(0.08),
                lineWidth: 1
            )
    }

    @ViewBuilder
    private var status: some View {
        if signal.excluded {
            // Unchecked: a hollow off-dot with an X.
            Image(systemName: "xmark")
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(.tertiary)
                .frame(width: 14, height: 14)
                .overlay(Circle().stroke(.tertiary, lineWidth: 1.5))
        }
        else if SignalBadgeStyle.awaitingChoice(signal) {
            // Nothing chosen: the count is how many there are to choose from.
            countCapsule(signal.options.count.formatted(), tone: .muted)
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

/// A signal with one value: clicking takes it in or out of the run. Hovering
/// shows a popover explaining where the value came from, the full untruncated
/// value, its state, and the click-to-toggle hint.
struct SignalBadge: View {
    let signal: BridgeToolbarSignal
    let onToggle: () -> Void
    let onRetry: () -> Void

    var body: some View {
        Button(action: onToggle) {
            SignalBadgeChip(signal: signal)
        }
        .buttonStyle(.plain)
        .hoverPopover(arrowEdge: .bottom) {
            SignalBadgePopover(signal: signal, onRetry: onRetry)
                .popoverEntrance(anchor: .top)
                .background { PopoverBehavior() }
        }
    }
}

/// The catalog: one badge over every number extracted from the candidate, each
/// with a checkbox. A folder can carry thirty of them, which as thirty chips
/// filled the sheet; as one control they are a list. At most one is checked —
/// checking another replaces it — because the run looks up the one it is told
/// to.
struct CatalogSignalBadge: View {
    let signal: BridgeToolbarSignal
    let onChoose: (_ value: String) -> Void

    var body: some View {
        Menu {
            ForEach(signal.options, id: \.value) { option in
                Toggle(
                    option.value,
                    isOn: Binding(
                        get: { option.chosen },
                        set: { _ in onChoose(option.value) }
                    )
                )
            }
        } label: {
            SignalBadgeChip(signal: signal)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .disabled(signal.options.isEmpty)
        .help(SignalBadgeStyle.stateLabel(for: signal))
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Signal badges") {
        VStack(alignment: .leading, spacing: 10) {
            SignalBadge(
                signal: BridgeToolbarSignal(
                    kind: .discId,
                    value: "Xx0Yy1Zz2Aa3Bb4Cc5",
                    origin: .discToc,
                    state: .found(count: 3),
                    excluded: false,
                    options: []
                ),
                onToggle: {},
                onRetry: {},
            )
            SignalBadge(
                signal: BridgeToolbarSignal(
                    kind: .barcode,
                    value: "0123456789012",
                    origin: .artwork,
                    state: .lookingUp,
                    excluded: false,
                    options: []
                ),
                onToggle: {},
                onRetry: {},
            )
            SignalBadge(
                signal: BridgeToolbarSignal(
                    kind: .barcode,
                    value: "0123456789012",
                    origin: .artwork,
                    state: .found(count: 4),
                    excluded: true,
                    options: []
                ),
                onToggle: {},
                onRetry: {},
            )
            CatalogSignalBadge(
                signal: BridgeToolbarSignal(
                    kind: .catalog,
                    value: nil,
                    origin: .folderName,
                    state: .skipped,
                    excluded: false,
                    options: [
                        BridgeSignalOption(
                            value: "WPCR-80001",
                            origin: .folderName,
                            chosen: false
                        ),
                        BridgeSignalOption(
                            value: "LBL 999",
                            origin: .artwork,
                            chosen: false
                        ),
                    ]
                ),
                onChoose: { _ in },
            )
            CatalogSignalBadge(
                signal: BridgeToolbarSignal(
                    kind: .catalog,
                    value: "WPCR-80001",
                    origin: .folderName,
                    state: .found(count: 1),
                    excluded: false,
                    options: [
                        BridgeSignalOption(
                            value: "WPCR-80001",
                            origin: .folderName,
                            chosen: true
                        )
                    ]
                ),
                onChoose: { _ in },
            )
        }
        .padding()
        .windowBackground()
    }
#endif
